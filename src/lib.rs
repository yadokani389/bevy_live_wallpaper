// If no backend feature is chosen and we're not on Windows, require an explicit choice.
#[cfg(all(
    not(feature = "wayland"),
    not(feature = "x11"),
    not(target_os = "windows")
))]
compile_error!("On non-Windows platforms, either the 'wayland' or 'x11' feature must be enabled.");

pub mod camera;
pub mod plugin;

#[cfg(feature = "wayland")]
pub mod wayland;

#[cfg(feature = "x11")]
pub mod x11;

#[cfg(target_os = "windows")]
mod windows_backend;

pub use plugin::LiveWallpaperPlugin;

pub use camera::LiveWallpaperCamera;

#[cfg(feature = "wayland")]
pub use wayland::surface::WaylandSurfaceHandles;

#[cfg(feature = "x11")]
pub use x11::surface::X11SurfaceHandles;

#[cfg(target_os = "windows")]
pub use windows_backend::{WallpaperTargetMonitor, WallpaperWindowsPlugin};
