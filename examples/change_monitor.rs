use bevy::prelude::*;
use bevy_live_wallpaper::LiveWallpaperPlugin;
use bevy_live_wallpaper::{WallpaperTargetMonitor, WallpaperWindowsPlugin};

fn main() {
    let mut app = App::new();

    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                decorations: false,
                ..default()
            }),
            ..default()
        }),
        LiveWallpaperPlugin.set(WallpaperWindowsPlugin {
            target_monitor: WallpaperTargetMonitor::Primary,
        }),
    ));

    app.add_systems(Startup, setup_scene)
        .add_systems(Update, change_monitor)
        .run();
}

fn setup_scene(mut commands: Commands) {
    commands.spawn(Camera2d);

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
    if *has_run || time.elapsed_secs() < 5.0 {
        return;
    }
    *has_run = true;
    *wallpaper_target = WallpaperTargetMonitor::Index(1);
}
