use bevy::prelude::*;

use crate::WallpaperTargetMonitor;

/// Main plugin to run the live wallpaper.
///
/// `target_monitor` controls which monitor(s) to use. It is honored by the
/// Wayland / Windows backends. X11 backend currently ignores this selection
/// (no multi-monitor targeting implemented there yet).
#[derive(Default)]
pub struct LiveWallpaperPlugin {
    pub target_monitor: WallpaperTargetMonitor,
}

impl Plugin for LiveWallpaperPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.target_monitor);

        #[cfg(feature = "wayland")]
        app.add_plugins(crate::wayland::backend::WaylandBackendPlugin);

        #[cfg(feature = "x11")]
        app.add_plugins(crate::x11::backend::X11BackendPlugin);

        #[cfg(target_os = "windows")]
        app.add_plugins(crate::windows_backend::WallpaperWindowsPlugin);
    }
}
