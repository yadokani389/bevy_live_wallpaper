use bevy::prelude::*;

/// Combined wallpaper surface extents in logical coordinates.
///
/// On Wayland, this is derived from layer-surface configure events and output
/// logical positions (xdg-output / wl_output). On other platforms it currently
/// stays at the default value unless implemented.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct WallpaperSurfaceInfo {
    /// Logical top-left of the wallpaper area (e.g., min x/y across outputs).
    pub offset_position: Vec2,
    /// Logical width/height of the wallpaper area.
    pub size: Vec2,
}

impl WallpaperSurfaceInfo {
    pub fn set(&mut self, offset_x: i32, offset_y: i32, width: u32, height: u32) {
        self.offset_position = Vec2::new(offset_x as f32, offset_y as f32);
        self.size = Vec2::new(width as f32, height as f32);
    }
}
