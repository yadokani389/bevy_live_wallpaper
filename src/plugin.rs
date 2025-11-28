use bevy::prelude::*;

use crate::{WallpaperPointerState, WallpaperSurfaceInfo, WallpaperTargetMonitor};

/// Main plugin to run the live wallpaper.
///
/// `target_monitor` controls which monitor(s) to use across all backends:
/// Wayland, X11 (RandR), and Windows.
#[derive(Default)]
pub struct LiveWallpaperPlugin {
    pub target_monitor: WallpaperTargetMonitor,
}

impl Plugin for LiveWallpaperPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.target_monitor)
            .init_resource::<WallpaperPointerState>()
            .init_resource::<WallpaperSurfaceInfo>();

        #[cfg(feature = "wayland")]
        app.add_plugins(crate::wayland::backend::WaylandBackendPlugin);

        #[cfg(feature = "x11")]
        app.add_plugins(crate::x11::backend::X11BackendPlugin);

        #[cfg(target_os = "windows")]
        app.add_plugins(crate::windows_backend::WallpaperWindowsPlugin);
    }
}
