// If no backend feature is chosen and we're not on Windows, require an explicit choice.
#[cfg(all(
    not(feature = "wayland"),
    not(feature = "x11"),
    not(target_os = "windows")
))]
compile_error!("On non-Windows platforms, either the 'wayland' or 'x11' feature must be enabled.");

pub mod camera;
pub mod input;
pub mod plugin;
pub mod surface_info;
pub mod target_monitor;

#[cfg(feature = "wayland")]
mod wayland;

#[cfg(feature = "x11")]
mod x11;

#[cfg(target_os = "windows")]
mod windows_backend;

pub use plugin::LiveWallpaperPlugin;

pub use camera::LiveWallpaperCamera;
pub use input::{PointerButton, PointerSample, WallpaperPointerState};
pub use surface_info::WallpaperSurfaceInfo;
pub use target_monitor::WallpaperTargetMonitor;

#[cfg(feature = "wayland")]
pub use wayland::surface::WaylandSurfaceHandles;

#[cfg(feature = "x11")]
pub use x11::surface::X11SurfaceHandles;
