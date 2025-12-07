use bevy::prelude::*;

use crate::{WallpaperPointerState, WallpaperSurfaceInfo, WallpaperTargetMonitor};

/// Main plugin to run the live wallpaper.
///
/// `target_monitor` controls which monitor(s) to use across all backends:
/// Wayland, X11 (RandR), and Windows.
#[derive(Default)]
pub struct LiveWallpaperPlugin {
    /// Selects which monitor(s) to render to (primary, index, or all).
    pub target_monitor: WallpaperTargetMonitor,
    /// Chooses how the wallpaper is presented.
    pub display_mode: WallpaperDisplayMode,
}

/// Selects wallpaper presentation mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WallpaperDisplayMode {
    /// Render directly to desktop surfaces (Wayland layer-shell, X11 root, Windows WorkerW).
    #[default]
    Wallpaper,
    /// Render inside a normal Bevy window while keeping wallpaper APIs available.
    Windowed,
}

impl Plugin for LiveWallpaperPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.target_monitor)
            .init_resource::<WallpaperPointerState>()
            .init_resource::<WallpaperSurfaceInfo>();

        match self.display_mode {
            WallpaperDisplayMode::Wallpaper => {
                #[cfg(feature = "wayland")]
                app.add_plugins(crate::wayland::backend::WaylandBackendPlugin);

                #[cfg(feature = "x11")]
                app.add_plugins(crate::x11::backend::X11BackendPlugin);

                #[cfg(target_os = "windows")]
                app.add_plugins(crate::windows_backend::WallpaperWindowsPlugin);
            }
            WallpaperDisplayMode::Windowed => {
                app.add_plugins(crate::windowed_backend::WindowedBackendPlugin);
            }
        }
    }
}
