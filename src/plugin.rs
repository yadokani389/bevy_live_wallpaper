use bevy::prelude::*;

#[derive(Default)]
pub struct LiveWallpaperPlugin;

impl Plugin for LiveWallpaperPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "wayland")]
        app.add_plugins(crate::wayland::backend::WaylandBackendPlugin);

        #[cfg(target_os = "windows")]
        app.add_plugins(crate::windows_backend::WindowsBackendPlugin);
    }
}
