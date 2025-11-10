use bevy::app::plugin_group;

plugin_group! {
    pub struct LiveWallpaperPlugin{
        #[cfg(feature = "wayland")]
        crate::wayland::backend:::WaylandBackendPlugin,
        #[custom(cfg(target_os = "windows"))]
        crate::windows_backend:::WallpaperWindowsPlugin,
    }
}
