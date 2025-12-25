use bevy::{
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
    /// Cached primary window entity.
    window_entity: Option<Entity>,
    /// Last known logical offset of the window within the virtual desktop.
    logical_offset: Vec2,
}

fn windowed_backend_system(
    mut state: ResMut<WindowedBackendState>,
    mut pointer_state: ResMut<WallpaperPointerState>,
    mut surface_info: ResMut<WallpaperSurfaceInfo>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut cursor_moved_events: MessageReader<CursorMoved>,
    mut mouse_button_events: MessageReader<MouseButtonInput>,
    mut window_resized_events: MessageReader<WindowResized>,
    mut window_moved_events: MessageReader<WindowMoved>,
) {
    let Some((window_entity, window)) = windows.iter().next() else {
        warn!("Windowed mode requires a primary window but none was found.");
        return;
    };

    state.window_entity = Some(window_entity);

    // Update cached logical offset from WindowMoved events.
    let scale_factor = window.scale_factor();
    for evt in window_moved_events.read() {
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
    for evt in window_resized_events.read() {
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

    let mut saw_cursor_event = false;
    let mut saw_button_event = false;

    // Track cursor movement in logical coordinates relative to desktop by adding offset.
    for evt in cursor_moved_events.read() {
        if evt.window != window_entity {
            continue;
        }
        saw_cursor_event = true;

        let global_position = evt.position + state.logical_offset;
        let prev_position = pointer_state
            .last
            .as_ref()
            .map(|p| p.position)
            .unwrap_or(global_position);

        let pressed = pointer_state
            .last
            .as_ref()
            .map(|p| p.pressed.clone())
            .unwrap_or_default();

        pointer_state.last = Some(PointerSample {
            output: None,
            position: global_position,
            delta: global_position - prev_position,
            last_button: None,
            pressed,
        });
    }

    // Mouse buttons are handled separately so clicks without movement still update state.
    for evt in mouse_button_events.read() {
        if evt.window != window_entity {
            continue;
        }
        saw_button_event = true;

        let mut pressed = pointer_state
            .last
            .as_ref()
            .map(|p| p.pressed.clone())
            .unwrap_or_default();

        match evt.state {
            ButtonState::Pressed => {
                pressed.insert(evt.button);
            }
            ButtonState::Released => {
                pressed.remove(&evt.button);
            }
        }

        let position = pointer_state
            .last
            .as_ref()
            .map(|p| p.position)
            .unwrap_or(state.logical_offset);

        pointer_state.last = Some(PointerSample {
            output: None,
            position,
            delta: Vec2::ZERO,
            last_button: Some(PointerButton {
                button: Some(evt.button),
                pressed: evt.state == ButtonState::Pressed,
            }),
            pressed,
        });
    }

    if !saw_cursor_event
        && !saw_button_event
        && let Some(sample) = pointer_state.last.as_mut()
    {
        sample.delta = Vec2::ZERO;
        sample.last_button = None;
    }
}
