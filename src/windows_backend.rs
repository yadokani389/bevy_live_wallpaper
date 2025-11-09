use bevy::prelude::*;
use bevy::window::RawHandleWrapper;
use bevy::winit::WINIT_WINDOWS;
use raw_window_handle::RawWindowHandle;
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GWL_STYLE, GetWindowLongW, SEND_MESSAGE_TIMEOUT_FLAGS,
    SWP_NOACTIVATE, SWP_NOZORDER, SendMessageTimeoutW, SetParent, SetWindowLongW, SetWindowPos,
    WS_CHILD, WS_OVERLAPPED, WS_POPUP,
};
use windows::core::PCWSTR;
use winit::window::Window as WinitWindow;

#[derive(Default)]
pub(crate) struct WindowsBackendPlugin;

impl Plugin for WindowsBackendPlugin {
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
            .insert_non_send_resource(workerw);
    }
}

// WINIT_WINDOWS is thread-local, so we add NonSendMarker to keep this system on the main thread.
fn attach_wallpaper_windows_system(
    windows: Query<(Entity, &RawHandleWrapper), With<Window>>,
    workerw: NonSend<HWND>,
) {
    WINIT_WINDOWS.with_borrow(|winit_windows| {
        for (entity, handle_wrapper) in windows.iter() {
            let raw_handle = handle_wrapper.get_window_handle();

            if let RawWindowHandle::Win32(win32_handle) = raw_handle {
                let hwnd = win32_handle.hwnd.get() as *mut std::ffi::c_void;

                unsafe {
                    let current_style = GetWindowLongW(HWND(hwnd), GWL_STYLE) as u32;
                    let new_style = (current_style & !(WS_POPUP.0 | WS_OVERLAPPED.0)) | WS_CHILD.0;
                    SetWindowLongW(HWND(hwnd), GWL_STYLE, new_style as i32);

                    SetParent(HWND(hwnd), Some(*workerw)).expect("Failed to set parent");
                };

                if let Some(winit_window) = winit_windows.get_window(entity) {
                    let (offset_x, offset_y) = virtual_desktop_offset(winit_window);

                    if let Some(monitor) = winit_window.current_monitor() {
                        let position = monitor.position();
                        let size = monitor.size();

                        let target_x = position.x + offset_x;
                        let target_y = position.y + offset_y;

                        unsafe {
                            SetWindowPos(
                                HWND(hwnd),
                                None,
                                target_x,
                                target_y,
                                size.width as i32,
                                size.height as i32,
                                SWP_NOACTIVATE | SWP_NOZORDER,
                            )
                            .expect("Failed to align window to monitor");
                        }
                    }
                }
            }
        }
    });
}

fn virtual_desktop_offset(window: &WinitWindow) -> (i32, i32) {
    let mut monitors = window.available_monitors();
    let mut min_x = 0;
    let mut min_y = 0;

    if let Some(first) = monitors.next() {
        let pos = first.position();
        min_x = pos.x;
        min_y = pos.y;

        for monitor in monitors {
            let pos = monitor.position();
            min_x = min_x.min(pos.x);
            min_y = min_y.min(pos.y);
        }
    }

    let offset_x = if min_x < 0 { -min_x } else { 0 };
    let offset_y = if min_y < 0 { -min_y } else { 0 };

    (offset_x, offset_y)
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
