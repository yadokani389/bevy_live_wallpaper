use bevy::prelude::*;

use crate::{WallpaperPointerState, WallpaperSurfaceInfo, WallpaperTargetMonitor};

/// Main plugin to run the live wallpaper.
#[derive(Default)]
pub struct LiveWallpaperPlugin {
    /// Selects which monitor(s) to render to (primary, index, or all).
    pub target_monitor: WallpaperTargetMonitor,
    /// Chooses how the wallpaper is presented.
    pub display_mode: WallpaperDisplayMode,
    /// (Linux only) Selects the backend to use for rendering.
    pub linux_backend: LinuxBackend,
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

/// Selects the Linux backend to use for rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinuxBackend {
    /// Automatically select the backend based on the environment (prefers Wayland).
    #[default]
    Auto,
    /// Force the Wayland backend.
    Wayland,
    /// Force the X11 backend.
    X11,
}

impl Plugin for LiveWallpaperPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.target_monitor)
            .init_resource::<WallpaperPointerState>()
            .init_resource::<WallpaperSurfaceInfo>();

        match self.display_mode {
            WallpaperDisplayMode::Wallpaper => {
                self.build_wallpaper_backend(app);
            }
            WallpaperDisplayMode::Windowed => {
                app.add_plugins(crate::windowed_backend::WindowedBackendPlugin);
            }
        }
    }
}

impl LiveWallpaperPlugin {
    fn build_wallpaper_backend(&self, app: &mut App) {
        #[cfg(target_os = "windows")]
        app.add_plugins(crate::windows_backend::WallpaperWindowsPlugin);

        #[cfg(all(not(target_os = "windows"), any(feature = "wayland", feature = "x11")))]
        self.build_linux_backend(app);
    }

    #[cfg(all(not(target_os = "windows"), any(feature = "wayland", feature = "x11")))]
    fn build_linux_backend(&self, app: &mut App) {
        const ONLY_WAYLAND: bool = cfg!(all(feature = "wayland", not(feature = "x11")));
        const ONLY_X11: bool = cfg!(all(feature = "x11", not(feature = "wayland")));

        let mut chosen_backend = self.linux_backend;

        if chosen_backend == LinuxBackend::Auto {
            if ONLY_WAYLAND {
                chosen_backend = LinuxBackend::Wayland;
            } else if ONLY_X11 {
                chosen_backend = LinuxBackend::X11;
            } else {
                let wayland_found = std::env::var("WAYLAND_DISPLAY").is_ok();
                if wayland_found {
                    chosen_backend = LinuxBackend::Wayland;
                } else {
                    chosen_backend = LinuxBackend::X11;
                }
            }
        }

        match chosen_backend {
            LinuxBackend::Wayland => {
                #[cfg(feature = "wayland")]
                {
                    info!("Using Wayland backend.");
                    app.add_plugins(crate::wayland::backend::WaylandBackendPlugin);
                }
                #[cfg(not(feature = "wayland"))]
                panic!(
                    "The Wayland backend was selected, but the 'wayland' feature is not enabled. Please explicitly choose a backend or enable it in your Cargo.toml."
                );
            }
            LinuxBackend::X11 => {
                #[cfg(feature = "x11")]
                {
                    info!("Using X11 backend.");
                    app.add_plugins(crate::x11::backend::X11BackendPlugin);
                }
                #[cfg(not(feature = "x11"))]
                panic!(
                    "The X11 backend was selected, but the 'x11' feature is not enabled. Please explicitly choose a backend or enable it in your Cargo.toml."
                );
            }
            LinuxBackend::Auto => unreachable!(),
        }
    }
}
