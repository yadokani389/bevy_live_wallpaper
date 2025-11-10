use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryMonitor, RawHandleWrapper};
use raw_window_handle::RawWindowHandle;
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GWL_STYLE, GetWindowLongW, SEND_MESSAGE_TIMEOUT_FLAGS,
    SendMessageTimeoutW, SetParent, SetWindowLongW, WS_CHILD, WS_OVERLAPPED, WS_POPUP,
};
use windows::core::PCWSTR;

#[derive(Default)]
pub struct WallpaperWindowsPlugin {
    pub target_monitor: WallpaperTargetMonitor,
}

impl Plugin for WallpaperWindowsPlugin {
    fn build(&self, app: &mut App) {
        let workerw = unsafe {
            let progman = FindWindowW(PCWSTR(to_wide_null("Progman").as_ptr()), None)
                .expect("Progman not found.");
            SendMessageTimeoutW(
                progman,
                0x052C,
                WPARAM(0),
                LPARAM(0),
                SEND_MESSAGE_TIMEOUT_FLAGS(0),
                1000,
                None,
            );

            FindWindowExW(
                Some(progman),
                None,
                PCWSTR(to_wide_null("WorkerW").as_ptr()),
                None,
            )
            .expect("workerw not found.")
        };
        app.add_systems(Startup, attach_wallpaper_windows_system)
            .insert_resource(self.target_monitor)
            .insert_non_send_resource(workerw);
    }
}

fn attach_wallpaper_windows_system(
    workerw: NonSend<HWND>,
    target_monitor: Res<WallpaperTargetMonitor>,
    monitors: Query<&Monitor>,
    primary_monitor: Single<&Monitor, With<PrimaryMonitor>>,
    windows: Query<(&RawHandleWrapper, &mut Window)>,
) {
    for (handle_wrapper, mut window) in windows {
        let raw_handle = handle_wrapper.get_window_handle();

        if let RawWindowHandle::Win32(win32_handle) = raw_handle {
            let hwnd = win32_handle.hwnd.get() as *mut std::ffi::c_void;

            unsafe {
                let current_style = GetWindowLongW(HWND(hwnd), GWL_STYLE) as u32;
                let new_style = (current_style & !(WS_POPUP.0 | WS_OVERLAPPED.0)) | WS_CHILD.0;
                SetWindowLongW(HWND(hwnd), GWL_STYLE, new_style as i32);

                SetParent(HWND(hwnd), Some(*workerw)).expect("Failed to set parent");
            };

            let Some((offset_x, offset_y)) = monitors
                .into_iter()
                .map(|m| (-m.physical_position.x, -m.physical_position.y))
                .reduce(|(x0, y0), (x1, y1)| (x0.max(x1), y0.max(y1)))
            else {
                return;
            };

            let Some(m) = (match *target_monitor {
                WallpaperTargetMonitor::Primary => Some(*primary_monitor),
                WallpaperTargetMonitor::Index(n) => monitors.iter().nth(n),
                WallpaperTargetMonitor::Entity(entity) => monitors.get(entity).ok(),
            }) else {
                return;
            };
            let pos = m.physical_position;
            let Some(scale) = monitors
                .into_iter()
                .map(|m| m.scale_factor as f32)
                .reduce(f32::max)
            else {
                return;
            };

            window
                .position
                .set(ivec2(offset_x + pos.x, offset_y + pos.y));
            window.resolution.set(
                m.physical_width as f32 / scale,
                m.physical_height as f32 / scale,
            );
        }
    }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Default, Clone, Copy, Resource)]
pub enum WallpaperTargetMonitor {
    /// Uses the primary monitor of the system.
    #[default]
    Primary,
    /// Uses the monitor with the specified index.
    Index(usize),
    /// Uses a given [`crate::monitor::Monitor`] entity.
    Entity(Entity),
}
