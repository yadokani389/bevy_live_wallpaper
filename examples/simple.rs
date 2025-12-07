use bevy::prelude::*;
use bevy_live_wallpaper::{LiveWallpaperCamera, LiveWallpaperPlugin};

fn main() {
    let mut app = App::new();

    // Platform-specific window adjustments for wallpaper mode.
    // On Wayland/X11, the primary window must be disabled; on Windows, use a borderless window.
    let mut window_plugin = WindowPlugin::default();

    #[cfg(any(feature = "wayland", feature = "x11"))]
    {
        window_plugin.primary_window = None;
        window_plugin.exit_condition = bevy::window::ExitCondition::DontExit;
    }

    #[cfg(target_os = "windows")]
    {
        window_plugin.primary_window = Some(Window {
            decorations: false,
            ..default()
        });
    }

    app.add_plugins(DefaultPlugins.set(window_plugin));

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
