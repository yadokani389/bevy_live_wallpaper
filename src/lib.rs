//! Bevy Live Wallpaper
//!
//! A Bevy plugin that renders your scene as the desktop wallpaper on Wayland,
//! X11, and Windows. Pick the matching backend feature (`wayland` or `x11`) on
//! Linux/BSD; Windows works with defaults.

#[cfg(all(
    not(feature = "wayland"),
    not(feature = "x11"),
    not(target_os = "windows")
))]
compile_error!(
    "On non-Windows platforms, at least one of the 'wayland' or 'x11' features must be enabled."
);

pub mod camera;
pub mod input;
pub mod plugin;
pub mod surface_info;
pub mod target_monitor;
mod windowed_backend;

#[cfg(feature = "wayland")]
mod wayland;

#[cfg(feature = "x11")]
mod x11;

#[cfg(target_os = "windows")]
mod windows_backend;

pub use plugin::{LinuxBackend, LiveWallpaperPlugin, WallpaperDisplayMode};

pub use camera::LiveWallpaperCamera;
pub use input::{PointerButton, PointerSample, WallpaperPointerState};
pub use surface_info::WallpaperSurfaceInfo;
pub use target_monitor::WallpaperTargetMonitor;

#[cfg(feature = "wayland")]
pub use wayland::surface::WaylandSurfaceHandles;

#[cfg(feature = "x11")]
pub use x11::surface::X11SurfaceHandles;
