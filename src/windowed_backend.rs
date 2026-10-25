use bevy::{
    ecs::system::SystemParam,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    window::{CursorMoved, PrimaryWindow, WindowMoved, WindowResized},
};

use crate::{PointerButton, PointerSample, WallpaperPointerState, WallpaperSurfaceInfo};

/// Backend that keeps wallpaper APIs working when rendering into a normal window.
pub(crate) struct WindowedBackendPlugin;

impl Plugin for WindowedBackendPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WindowedBackendState>()
            .add_systems(Update, windowed_backend_system);
    }
}

#[derive(Default, Resource)]
struct WindowedBackendState {
    /// Last known logical offset of the window within the virtual desktop.
    logical_offset: Vec2,
}

#[derive(SystemParam)]
struct WindowedBackendParams<'w, 's> {
    windows: Query<'w, 's, (Entity, &'static Window), With<PrimaryWindow>>,
    cursor_moved_events: MessageReader<'w, 's, CursorMoved>,
    mouse_button_events: MessageReader<'w, 's, MouseButtonInput>,
    window_resized_events: MessageReader<'w, 's, WindowResized>,
    window_moved_events: MessageReader<'w, 's, WindowMoved>,
}

fn windowed_backend_system(
    mut state: ResMut<WindowedBackendState>,
    mut pointer_state: ResMut<WallpaperPointerState>,
    mut surface_info: ResMut<WallpaperSurfaceInfo>,
    mut params: WindowedBackendParams,
) {
    let Some((window_entity, window)) = params.windows.iter().next() else {
        warn!("Windowed mode requires a primary window but none was found.");
        return;
    };

    // Update cached logical offset from WindowMoved events.
    let scale_factor = window.scale_factor();
    for evt in params.window_moved_events.read() {
        if evt.window != window_entity {
            continue;
        }
        state.logical_offset = Vec2::new(
            evt.position.x as f32 / scale_factor,
            evt.position.y as f32 / scale_factor,
        );
    }

    // Apply size/offset to surface info; prefer event size when present, fallback to current window size.
    let mut latest_width = window.width();
    let mut latest_height = window.height();
    for evt in params.window_resized_events.read() {
        if evt.window != window_entity {
            continue;
        }
        latest_width = evt.width;
        latest_height = evt.height;
    }

    surface_info.set(
        state.logical_offset.x.floor() as i32,
        state.logical_offset.y.floor() as i32,
        latest_width.max(1.0) as u32,
        latest_height.max(1.0) as u32,
    );

    if let Some(sample) = pointer_state.last.as_mut() {
        sample.delta = Vec2::ZERO;
        sample.last_button = None;
    }

    // Track cursor movement in logical coordinates relative to desktop by adding offset.
    for evt in params.cursor_moved_events.read() {
        if evt.window != window_entity {
            continue;
        }

        let global_position = evt.position + state.logical_offset;
        let sample = pointer_state.last.get_or_insert_with(|| PointerSample {
            position: global_position,
            ..default()
        });
        sample.output = None;
        sample.delta = global_position - sample.position;
        sample.position = global_position;
        sample.last_button = None;
    }

    // Mouse buttons are handled separately so clicks without movement still update state.
    for evt in params.mouse_button_events.read() {
        if evt.window != window_entity {
            continue;
        }

        let sample = pointer_state.last.get_or_insert_with(|| PointerSample {
            position: state.logical_offset,
            ..default()
        });
        match evt.state {
            ButtonState::Pressed => {
                sample.pressed.insert(evt.button);
            }
            ButtonState::Released => {
                sample.pressed.remove(&evt.button);
            }
        }

        sample.output = None;
        sample.delta = Vec2::ZERO;
        sample.last_button = Some(PointerButton {
            button: Some(evt.button),
            pressed: evt.state == ButtonState::Pressed,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_updates_keep_held_buttons_and_clear_transient_state() {
        let mut app = App::new();
        app.init_resource::<WallpaperPointerState>()
            .init_resource::<WallpaperSurfaceInfo>()
            .add_message::<CursorMoved>()
            .add_message::<MouseButtonInput>()
            .add_message::<WindowMoved>()
            .add_message::<WindowResized>()
            .add_plugins(WindowedBackendPlugin);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.world_mut().write_message(WindowMoved {
            window,
            position: IVec2::new(-100, 50),
        });
        app.world_mut().write_message(MouseButtonInput {
            window,
            button: MouseButton::Left,
            state: ButtonState::Pressed,
        });
        app.update();
        let sample = app
            .world()
            .resource::<WallpaperPointerState>()
            .last
            .as_ref()
            .unwrap();
        assert_eq!(sample.position, Vec2::new(-100.0, 50.0));
        assert!(sample.pressed.contains(&MouseButton::Left));
        assert!(sample.last_button.unwrap().pressed);

        app.world_mut().write_message(CursorMoved {
            window,
            position: Vec2::new(30.0, 40.0),
            delta: None,
        });
        app.update();
        let sample = app
            .world()
            .resource::<WallpaperPointerState>()
            .last
            .as_ref()
            .unwrap();
        assert_eq!(sample.position, Vec2::new(-70.0, 90.0));
        assert_eq!(sample.delta, Vec2::new(30.0, 40.0));
        assert!(sample.pressed.contains(&MouseButton::Left));
        assert!(sample.last_button.is_none());

        app.update();
        let sample = app
            .world()
            .resource::<WallpaperPointerState>()
            .last
            .as_ref()
            .unwrap();
        assert_eq!(sample.delta, Vec2::ZERO);
        assert!(sample.pressed.contains(&MouseButton::Left));
        app.world_mut().write_message(MouseButtonInput {
            window,
            button: MouseButton::Left,
            state: ButtonState::Released,
        });
        app.update();
        let sample = app
            .world()
            .resource::<WallpaperPointerState>()
            .last
            .as_ref()
            .unwrap();
        assert!(sample.pressed.is_empty());
        assert!(!sample.last_button.unwrap().pressed);
    }
}
