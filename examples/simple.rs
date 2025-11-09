use bevy::prelude::*;
use bevy::window::WindowMode;
use bevy_live_wallpaper::LiveWallpaperPlugin;

fn main() {
    let mut app = App::new();

    // Platform-specific plugin setup
    #[cfg(target_os = "linux")]
    {
        // On Wayland, we can't have a primary window
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..default()
        }));
    }
    #[cfg(not(target_os = "linux"))]
    {
        // On Windows we must start as BorderlessFullscreen so the WorkerW child covers the monitor.

        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                mode: WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
                ..default()
            }),
            ..default()
        }));
    }

    app.add_plugins(LiveWallpaperPlugin)
        .add_systems(Startup, setup_scene)
        .run();
}

fn setup_scene(mut commands: Commands) {
    // Spawn a camera.
    let mut camera = commands.spawn(Camera2d);

    // On Wayland, it needs the LiveWallpaperCamera component
    // to be picked up by the plugin.
    #[cfg(target_os = "linux")]
    camera.insert(bevy_live_wallpaper::LiveWallpaperCamera);

    // ... spawn your scene entities here ...
    commands.spawn((
        Sprite::from_color(Color::srgb(0.15, 0.4, 0.85), Vec2::splat(1600.0)),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}
