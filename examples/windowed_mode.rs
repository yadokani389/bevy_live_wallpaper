//! Minimal 2D example that runs LiveWallpaper in windowed mode.
//!
//! Run on Wayland (Linux/BSD):
//! `cargo run --features=wayland --example windowed_mode`
//!
//! Run on Windows:
//! `cargo run --example windowed_mode`

use bevy::{math::Isometry2d, prelude::*};
use bevy_live_wallpaper::{
    LiveWallpaperCamera, LiveWallpaperPlugin, WallpaperDisplayMode, WallpaperPointerState,
    WallpaperSurfaceInfo,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(LiveWallpaperPlugin {
            display_mode: WallpaperDisplayMode::Windowed,
            ..default()
        })
        .add_systems(Startup, setup)
        .add_systems(Update, (debug_overlay, debug_cursor))
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn((Camera2d, LiveWallpaperCamera));

    commands.spawn((
        Sprite::from_color(Color::srgb(0.2, 0.6, 0.9), Vec2::splat(300.0)),
        Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
    ));
}

fn debug_overlay(mut gizmos: Gizmos, surface: Res<WallpaperSurfaceInfo>) {
    // Draw the wallpaper surface rectangle (origin-centered for visibility).
    let rect = Rect::from_center_size(Vec2::ZERO, surface.size * 0.9);
    gizmos.rect_2d(
        Isometry2d::from_translation(rect.center()),
        rect.size(),
        Color::srgb(1.0, 1.0, 0.0),
    );
}

fn debug_cursor(
    mut gizmos: Gizmos,
    surface: Res<WallpaperSurfaceInfo>,
    pointer: Res<WallpaperPointerState>,
) {
    let Some(sample) = &pointer.last else {
        return;
    };

    // Convert to surface-local, center-origin coordinates (Y up).
    let mut local = sample.position - surface.offset_position;
    local.x -= surface.size.x / 2.0;
    local.y = surface.size.y / 2.0 - local.y;

    let delta_world = Vec2::new(sample.delta.x, -sample.delta.y);
    let prev = local - delta_world;

    gizmos.circle_2d(local, 6.0, Color::srgb(0.2, 0.8, 0.3));
    gizmos.line_2d(prev, local, Color::srgb(0.7, 0.7, 1.0));
}
