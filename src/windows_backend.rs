use bevy::prelude::*;
use bevy::window::RawHandleWrapper;
use raw_window_handle::RawWindowHandle;
use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GWL_STYLE, GetWindowLongW, SEND_MESSAGE_TIMEOUT_FLAGS,
    SendMessageTimeoutW, SetParent, SetWindowLongW, WS_CHILD, WS_OVERLAPPED, WS_POPUP,
};
use windows::core::PCWSTR;

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
        app.add_systems(Startup, get_hwnd_system)
            .insert_non_send_resource(workerw);
    }
}

fn get_hwnd_system(windows: Query<&RawHandleWrapper, With<Window>>, workerw: NonSend<HWND>) {
    for handle_wrapper in windows.iter() {
        let raw_handle = handle_wrapper.get_window_handle();

        if let RawWindowHandle::Win32(win32_handle) = raw_handle {
            let hwnd = win32_handle.hwnd.get() as *mut std::ffi::c_void;

            unsafe {
                let current_style = GetWindowLongW(HWND(hwnd), GWL_STYLE) as u32;
                let new_style = (current_style & !(WS_POPUP.0 | WS_OVERLAPPED.0)) | WS_CHILD.0;
                SetWindowLongW(HWND(hwnd), GWL_STYLE, new_style as i32);

                SetParent(HWND(hwnd), Some(*workerw)).expect("Failed to set parent");
            };
        }
    }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
