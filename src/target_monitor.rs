use bevy::prelude::Resource;

/// Selects which monitor(s) should display the wallpaper.
#[derive(Default, Clone, Copy, Debug, Resource)]
pub enum WallpaperTargetMonitor {
    /// Uses the primary monitor of the system.
    #[default]
    Primary,
    /// Uses the monitor with the specified index.
    Index(usize),
    /// Uses all monitors as one large logical desktop.
    All,
}
