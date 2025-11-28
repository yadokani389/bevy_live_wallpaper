use bevy::prelude::*;
use bevy_live_wallpaper::{LiveWallpaperCamera, LiveWallpaperPlugin};

fn main() {
    let mut app = App::new();

    // Platform-specific plugin setup
    #[cfg(any(feature = "wayland", feature = "x11"))]
    {
        // On Wayland/X11, we can't have a primary window
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..default()
        }));
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows we must start as BorderlessFullscreen so the WorkerW child covers the monitor.
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                decorations: false,
                ..default()
            }),
            ..default()
        }));
    }

    app.add_plugins(LiveWallpaperPlugin::default());

    app.add_systems(Startup, setup_scene).run();
}

fn setup_scene(mut commands: Commands) {
    // Spawn a camera. On Wayland/X11 this component is required; on Windows
    // it is optional but harmless to keep for consistency.
    commands.spawn((Camera2d, LiveWallpaperCamera));

    // ... spawn your scene entities here ...
    commands.spawn((
        Sprite::from_color(Color::srgb(0.15, 0.4, 0.85), Vec2::splat(1600.0)),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}
