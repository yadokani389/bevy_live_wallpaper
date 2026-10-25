use bevy::{prelude::*, window::Monitor};

use crate::{WallpaperSurfaceInfo, WallpaperTargetMonitor};

#[derive(Clone, Copy, Debug)]
pub(crate) struct MonitorLayout {
    pub position: IVec2,
    pub size: Vec2,
    desktop_origin: IVec2,
    scale: f32,
}

impl MonitorLayout {
    pub(crate) fn new(
        monitors: &[&Monitor],
        primary: Option<&Monitor>,
        target: WallpaperTargetMonitor,
    ) -> Option<Self> {
        let desktop_origin = monitors
            .iter()
            .map(|monitor| monitor.physical_position)
            .reduce(IVec2::min)?;
        let desktop_end = monitors
            .iter()
            .map(|monitor| monitor.physical_position + monitor.physical_size().as_ivec2())
            .reduce(IVec2::max)?;
        let scale = monitors
            .iter()
            .map(|monitor| monitor.scale_factor as f32)
            .reduce(f32::max)?;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let bounds = |monitor: &Monitor| {
            (
                monitor.physical_position,
                monitor.physical_size().as_ivec2(),
            )
        };
        let (origin, size) = match target {
            WallpaperTargetMonitor::All => (desktop_origin, desktop_end - desktop_origin),
            WallpaperTargetMonitor::Primary => bounds(primary?),
            WallpaperTargetMonitor::Index(index) => bounds(monitors.get(index)?),
        };
        Some(Self {
            position: origin - desktop_origin,
            size: size.as_vec2() / scale,
            desktop_origin,
            scale,
        })
    }

    pub(crate) fn logical_position(&self, physical: IVec2) -> Vec2 {
        (physical - self.desktop_origin).as_vec2() / self.scale
    }

    pub(crate) fn surface_info(&self) -> WallpaperSurfaceInfo {
        WallpaperSurfaceInfo {
            offset_position: (self.position.as_vec2() / self.scale).floor(),
            size: self.size.ceil().max(Vec2::ONE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(position: IVec2, size: UVec2, scale_factor: f64) -> Monitor {
        Monitor {
            name: None,
            physical_position: position,
            physical_width: size.x,
            physical_height: size.y,
            scale_factor,
            refresh_rate_millihertz: None,
            video_modes: Vec::new(),
        }
    }

    #[test]
    fn layout_uses_the_same_origin_and_scale_for_each_target() {
        let left = monitor(IVec2::new(-1920, -200), UVec2::new(1920, 1080), 1.0);
        let primary = monitor(IVec2::ZERO, UVec2::new(2560, 1440), 2.0);
        let monitors = [&left, &primary];
        let all = MonitorLayout::new(&monitors, None, WallpaperTargetMonitor::All).unwrap();
        assert_eq!(all.position, IVec2::ZERO);
        assert_eq!(all.size, Vec2::new(2240.0, 820.0));
        let selected =
            MonitorLayout::new(&monitors, Some(&primary), WallpaperTargetMonitor::Primary).unwrap();
        assert_eq!(selected.position, IVec2::new(1920, 200));
        assert_eq!(selected.size, Vec2::new(1280.0, 720.0));
        assert_eq!(
            selected.surface_info().offset_position,
            Vec2::new(960.0, 100.0)
        );
        assert_eq!(
            selected.logical_position(IVec2::ZERO),
            selected.surface_info().offset_position
        );
        let indexed =
            MonitorLayout::new(&monitors, None, WallpaperTargetMonitor::Index(0)).unwrap();
        assert_eq!(indexed.position, IVec2::ZERO);
        assert_eq!(indexed.size, Vec2::new(960.0, 540.0));
        assert!(
            MonitorLayout::new(&monitors, Some(&primary), WallpaperTargetMonitor::Index(2))
                .is_none()
        );
        assert!(MonitorLayout::new(&monitors, None, WallpaperTargetMonitor::Primary).is_none());
        assert!(MonitorLayout::new(&[], None, WallpaperTargetMonitor::All).is_none());
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let invalid = monitor(IVec2::ZERO, UVec2::new(1920, 1080), scale);
            assert!(MonitorLayout::new(&[&invalid], None, WallpaperTargetMonitor::All).is_none());
        }
    }
}
