use bevy::prelude::*;

/// Marks a camera whose output should be redirected to the wallpaper surface.
/// This component is used by non-windowed backends such as Wayland and X11.
#[derive(Component, Default)]
pub struct LiveWallpaperCamera;
