use std::collections::HashSet;

use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryMonitor, RawHandleWrapper};
use raw_window_handle::RawWindowHandle;
use windows::Win32::Foundation::POINT;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, FindWindowExW, FindWindowW, GWL_EXSTYLE, GWL_STYLE,
    GetClassNameW, GetCursorPos, GetWindowLongW, PostMessageW, SEND_MESSAGE_TIMEOUT_FLAGS,
    SendMessageTimeoutW, SetParent, SetWindowLongW, WM_CLOSE, WS_CHILD, WS_EX_APPWINDOW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_OVERLAPPED, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR};

use crate::{
    PointerSample, WallpaperPointerState, WallpaperSurfaceInfo, WallpaperTargetMonitor,
    input::detect_last_button, monitor_layout::MonitorLayout,
};

#[derive(Default)]
pub(crate) struct WallpaperWindowsPlugin;

impl Plugin for WallpaperWindowsPlugin {
    fn build(&self, app: &mut App) {
        let workerw = find_workerw().expect("workerw not found.");
        app.add_systems(Startup, attach_wallpaper_windows_system)
            .add_systems(Update, update_wallpaper_system)
            .insert_non_send(workerw);
    }
}

fn attach_wallpaper_windows_system(
    workerw: NonSend<HWND>,
    handle_wrappers: Query<&RawHandleWrapper, With<Window>>,
) {
    for handle_wrapper in handle_wrappers {
        let raw_handle = handle_wrapper.get_window_handle();

        if let RawWindowHandle::Win32(win32_handle) = raw_handle {
            let hwnd = win32_handle.hwnd.get() as *mut std::ffi::c_void;

            close_duplicate_instances(*workerw, HWND(hwnd));

            unsafe {
                let current_style = GetWindowLongW(HWND(hwnd), GWL_STYLE) as u32;
                let new_style = (current_style & !(WS_POPUP.0 | WS_OVERLAPPED.0)) | WS_CHILD.0;
                SetWindowLongW(HWND(hwnd), GWL_STYLE, new_style as i32);

                let current_ex_style = GetWindowLongW(HWND(hwnd), GWL_EXSTYLE) as u32;
                let cleared = current_ex_style & !WS_EX_APPWINDOW.0;
                let ex_style = cleared | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0;
                SetWindowLongW(HWND(hwnd), GWL_EXSTYLE, ex_style as i32);

                SetParent(HWND(hwnd), Some(*workerw)).expect("Failed to set parent");
            };
        }
    }
}

fn update_wallpaper_system(
    target_monitor: Res<WallpaperTargetMonitor>,
    monitors_query: Query<(&Monitor, Has<PrimaryMonitor>)>,
    mut window: Single<&mut Window>,
    mut pointer_state: ResMut<WallpaperPointerState>,
    mut surface_info: ResMut<WallpaperSurfaceInfo>,
) {
    let monitors: Vec<&Monitor> = monitors_query.iter().map(|(monitor, _)| monitor).collect();
    let primary = monitors_query
        .iter()
        .find(|(_, primary)| *primary)
        .map(|(monitor, _)| monitor);
    let Some(layout) = MonitorLayout::new(&monitors, primary, *target_monitor) else {
        return;
    };

    let position = bevy::window::WindowPosition::At(layout.position);
    if window.position != position {
        window.position = position;
    }
    let mut resolution = window.resolution.clone();
    resolution.set(layout.size.x, layout.size.y);
    if window.resolution != resolution {
        window.resolution = resolution;
    }
    surface_info.set_if_neq(layout.surface_info());

    let Some((cursor_x, cursor_y)) = current_cursor_position() else {
        return;
    };

    let logical_position = layout.logical_position(IVec2::new(cursor_x, cursor_y));

    let pressed = pressed_buttons();
    let last_button = detect_last_button(pointer_state.last.as_ref().map(|s| &s.pressed), &pressed);
    let prev_position = pointer_state
        .last
        .as_ref()
        .map(|s| s.position)
        .unwrap_or(logical_position);

    let output = output_for_position(&monitors, cursor_x, cursor_y);

    pointer_state.last = Some(PointerSample {
        output,
        position: logical_position,
        delta: logical_position - prev_position,
        last_button,
        pressed,
    });
}

fn current_cursor_position() -> Option<(i32, i32)> {
    unsafe {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_ok() {
            Some((point.x, point.y))
        } else {
            None
        }
    }
}

fn output_for_position(monitors: &[&Monitor], x: i32, y: i32) -> Option<u32> {
    monitors
        .iter()
        .enumerate()
        .find(|(_, monitor)| {
            let pos = monitor.physical_position;
            let width = monitor.physical_width as i32;
            let height = monitor.physical_height as i32;
            x >= pos.x && x < pos.x + width && y >= pos.y && y < pos.y + height
        })
        .map(|(idx, _)| idx as u32)
}

fn pressed_buttons() -> HashSet<MouseButton> {
    let mut set = HashSet::new();
    unsafe {
        if GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 {
            set.insert(MouseButton::Left);
        }
        if GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0 {
            set.insert(MouseButton::Right);
        }
        if GetAsyncKeyState(VK_MBUTTON.0 as i32) < 0 {
            set.insert(MouseButton::Middle);
        }
    }
    set
}

fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn find_workerw() -> Option<HWND> {
    let progman = unsafe { FindWindowW(PCWSTR(to_wide_null("Progman").as_ptr()), None).ok()? };
    unsafe {
        let _ = SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0),
            LPARAM(0),
            SEND_MESSAGE_TIMEOUT_FLAGS(0),
            1000,
            None,
        );
    }

    find_workerw_for_progman(progman).or_else(find_workerw_from_desktop)
}

fn find_workerw_for_progman(progman: HWND) -> Option<HWND> {
    let mut state = WorkerFinder::default();
    unsafe {
        _ = EnumChildWindows(
            Some(progman),
            Some(enum_child_worker_proc),
            LPARAM(&mut state as *mut _ as isize),
        );
    }
    state.worker
}

fn find_workerw_from_desktop() -> Option<HWND> {
    let mut state = WorkerFinder::default();
    unsafe {
        _ = EnumWindows(
            Some(enum_windows_worker_proc),
            LPARAM(&mut state as *mut _ as isize),
        );
    }
    state.worker
}

#[derive(Default)]
struct WorkerFinder {
    worker: Option<HWND>,
}

unsafe extern "system" fn enum_child_worker_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = unsafe { &mut *(lparam.0 as *mut WorkerFinder) };
    if state.worker.is_some() {
        return BOOL(0);
    }
    if is_class(hwnd, "WorkerW") {
        state.worker = Some(hwnd);
        return BOOL(0);
    }
    BOOL(1)
}

unsafe extern "system" fn enum_windows_worker_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = unsafe { &mut *(lparam.0 as *mut WorkerFinder) };
    if state.worker.is_some() {
        return BOOL(0);
    }

    let shell = unsafe {
        FindWindowExW(
            Some(hwnd),
            None,
            PCWSTR(to_wide_null("SHELLDLL_DefView").as_ptr()),
            None,
        )
    };
    if shell.is_err() {
        return BOOL(1);
    }

    let worker = unsafe {
        FindWindowExW(
            None,
            Some(hwnd),
            PCWSTR(to_wide_null("WorkerW").as_ptr()),
            None,
        )
    };
    if let Ok(worker) = worker {
        state.worker = Some(worker);
        return BOOL(0);
    }

    BOOL(1)
}

fn is_class(hwnd: HWND, class: &str) -> bool {
    let target: Vec<u16> = class.encode_utf16().collect();
    window_class_utf16(hwnd)
        .map(|name| name == target)
        .unwrap_or(false)
}

fn window_class_utf16(hwnd: HWND) -> Option<Vec<u16>> {
    unsafe {
        let mut buffer = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut buffer);
        if len == 0 {
            return None;
        }
        Some(buffer[..len as usize].to_vec())
    }
}

fn close_duplicate_instances(workerw: HWND, current_hwnd: HWND) {
    let Some(class_name) = window_class_utf16(current_hwnd) else {
        return;
    };
    let mut state = DuplicateCleanupState {
        class_name,
        current: current_hwnd,
    };
    unsafe {
        _ = EnumChildWindows(
            Some(workerw),
            Some(enum_duplicate_cleanup_proc),
            LPARAM(&mut state as *mut _ as isize),
        );
    }
}

struct DuplicateCleanupState {
    class_name: Vec<u16>,
    current: HWND,
}

unsafe extern "system" fn enum_duplicate_cleanup_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = unsafe { &*(lparam.0 as *mut DuplicateCleanupState) };
    if hwnd == state.current {
        return BOOL(1);
    }
    if let Some(class_name) = window_class_utf16(hwnd)
        && class_name == state.class_name
    {
        unsafe {
            _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
    BOOL(1)
}
