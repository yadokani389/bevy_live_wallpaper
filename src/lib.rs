// If no backend feature is chosen and we're not on Windows, require an explicit choice.
#[cfg(all(not(feature = "wayland"), not(target_os = "windows")))]
compile_error!("On non-Windows platforms, the 'wayland' feature must be enabled.");

pub mod plugin;

#[cfg(feature = "wayland")]
pub mod wayland;

#[cfg(target_os = "windows")]
mod windows_backend;

pub use plugin::LiveWallpaperPlugin;

#[cfg(feature = "wayland")]
pub use wayland::{LiveWallpaperCamera, surface::WaylandSurfaceHandles};

#[cfg(target_os = "windows")]
pub use windows_backend::{WallpaperTargetMonitor, WallpaperWindowsPlugin};
