//! Pointer input demo for `bevy_live_wallpaper`.

use bevy::prelude::*;
use bevy_live_wallpaper::{
    LiveWallpaperCamera, LiveWallpaperPlugin, WallpaperPointerState, WallpaperSurfaceInfo,
};

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .build(),
        )
        .add_plugins(LiveWallpaperPlugin {
            target_monitor: bevy_live_wallpaper::WallpaperTargetMonitor::All,
        })
        .add_systems(Startup, spawn_camera)
        .add_systems(Update, handle_pointer_state)
        .run();
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera2d, LiveWallpaperCamera));
}

fn handle_pointer_state(
    state: Res<WallpaperPointerState>,
    mut gizmos: Gizmos,
    surface: Res<WallpaperSurfaceInfo>,
) {
    if let Some(sample) = &state.last {
        println!(
            "Output {:?}: position={:?}, delta={:?}, pressed={:?}, last_button={:?}",
            sample.output, sample.position, sample.delta, sample.pressed, sample.last_button
        );
        let mut position = sample.position - surface.offset_position;
        position.y *= -1.;
        position.x -= surface.size.x / 2.0;
        position.y += surface.size.y / 2.0;

        let mut prev = position;
        prev.x -= sample.delta.x;
        prev.y += sample.delta.y;
        let color = sample
            .last_button
            .map(|btn| {
                if btn.pressed {
                    Color::srgb(0.1, 0.8, 0.3)
                } else {
                    Color::srgb(0.9, 0.2, 0.2)
                }
            })
            .unwrap_or(Color::WHITE);
        gizmos.circle_2d(position, 8.0, color);
        gizmos.line_2d(prev, position, Color::srgb(0.6, 0.6, 1.0));

        let mut radius = 12.0;
        for button in &sample.pressed {
            let ring_color = match button {
                MouseButton::Left => Color::srgb(0.2, 0.7, 1.0),
                MouseButton::Right => Color::srgb(1.0, 0.6, 0.2),
                MouseButton::Middle => Color::srgb(0.8, 0.8, 0.2),
                MouseButton::Back | MouseButton::Forward => Color::srgb(0.6, 0.4, 1.0),
                MouseButton::Other(_) => Color::srgb(0.7, 0.7, 0.7),
            };
            gizmos.circle_2d(position, radius, ring_color);
            radius += 4.0;
        }
    }
}
