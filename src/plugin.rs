use bevy::app::plugin_group;

plugin_group! {
    pub struct LiveWallpaperPlugin{
        #[cfg(feature = "wayland")]
        crate::wayland::backend:::WaylandBackendPlugin,
        #[cfg(feature = "x11")]
        crate::x11::backend:::X11BackendPlugin,
        #[custom(cfg(target_os = "windows"))]
        crate::windows_backend:::WallpaperWindowsPlugin,
    }
}
