//! Change the target monitor at runtime.
//! Works on Windows and Wayland (with the `wayland` feature).

use bevy::prelude::*;
use bevy_live_wallpaper::{LiveWallpaperCamera, LiveWallpaperPlugin, WallpaperTargetMonitor};

fn main() {
    let mut app = App::new();

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

    app.add_systems(Startup, setup_scene)
        .add_systems(Update, change_monitor)
        .run();
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((Camera2d, LiveWallpaperCamera));

    commands.spawn((
        Sprite::from_color(Color::srgb(0.15, 0.4, 0.85), Vec2::splat(1600.0)),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}

fn change_monitor(
    mut wallpaper_target: ResMut<WallpaperTargetMonitor>,
    mut has_run: Local<bool>,
    time: Res<Time>,
) {
    // Switch after 5 seconds once.
    if *has_run || time.elapsed_secs() < 5.0 {
        return;
    }
    *has_run = true;

    *wallpaper_target = WallpaperTargetMonitor::All;
}
