use crate::{
    PointerButton, PointerSample, WallpaperPointerState, WallpaperSurfaceInfo,
    WallpaperTargetMonitor,
};
use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryMonitor, RawHandleWrapper};
use raw_window_handle::RawWindowHandle;
use std::collections::HashSet;
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

#[derive(Default)]
pub(crate) struct WallpaperWindowsPlugin;

impl Plugin for WallpaperWindowsPlugin {
    fn build(&self, app: &mut App) {
        let workerw = find_workerw().expect("workerw not found.");
        app.add_systems(Startup, attach_wallpaper_windows_system)
            .add_systems(
                Update,
                (
                    update_window_position_and_size_system
                        .run_if(resource_changed::<WallpaperTargetMonitor>),
                    update_pointer_and_surface_info_system,
                )
                    .chain(),
            )
            .insert_non_send_resource(workerw);
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

fn update_window_position_and_size_system(
    target_monitor: Res<WallpaperTargetMonitor>,
    monitors: Query<&Monitor>,
    primary_monitor: Single<&Monitor, With<PrimaryMonitor>>,
    mut window: Single<&mut Window>,
) {
    let Some((offset_x, offset_y)) = monitors
        .into_iter()
        .map(|m| (-m.physical_position.x, -m.physical_position.y))
        .reduce(|(x0, y0), (x1, y1)| (x0.max(x1), y0.max(y1)))
    else {
        return;
    };
    let Some(scale) = monitors
        .into_iter()
        .map(|m| m.scale_factor as f32)
        .reduce(f32::max)
    else {
        return;
    };

    let (pos_x, pos_y, width, height) = if let WallpaperTargetMonitor::All = *target_monitor {
        let Some((max_x, max_y)) = monitors
            .into_iter()
            .map(|m| {
                (
                    m.physical_position.x + m.physical_width as i32,
                    m.physical_position.y + m.physical_height as i32,
                )
            })
            .reduce(|(x0, y0), (x1, y1)| (x0.max(x1), y0.max(y1)))
        else {
            return;
        };
        (
            0,
            0,
            (max_x + offset_x) as f32 / scale,
            (max_y + offset_y) as f32 / scale,
        )
    } else {
        let Some(m) = (match *target_monitor {
            WallpaperTargetMonitor::Primary => Some(*primary_monitor),
            WallpaperTargetMonitor::Index(n) => monitors.iter().nth(n),
            WallpaperTargetMonitor::All => None,
        }) else {
            return;
        };
        let pos = m.physical_position;

        (
            pos.x + offset_x,
            pos.y + offset_y,
            m.physical_width as f32 / scale,
            m.physical_height as f32 / scale,
        )
    };

    window.position.set(ivec2(pos_x, pos_y));
    window.resolution.set(width, height);
}

fn update_pointer_and_surface_info_system(
    target_monitor: Res<WallpaperTargetMonitor>,
    monitors_query: Query<&Monitor>,
    primary_monitor: Single<&Monitor, With<PrimaryMonitor>>,
    mut pointer_state: ResMut<WallpaperPointerState>,
    mut surface_info: ResMut<WallpaperSurfaceInfo>,
) {
    let monitors: Vec<&Monitor> = monitors_query.iter().collect();
    if monitors.is_empty() {
        return;
    }

    let Some(min_x) = monitors.iter().map(|m| m.physical_position.x).min() else {
        return;
    };
    let Some(min_y) = monitors.iter().map(|m| m.physical_position.y).min() else {
        return;
    };
    let Some(max_x) = monitors
        .iter()
        .map(|m| m.physical_position.x + m.physical_width as i32)
        .max()
    else {
        return;
    };
    let Some(max_y) = monitors
        .iter()
        .map(|m| m.physical_position.y + m.physical_height as i32)
        .max()
    else {
        return;
    };
    let Some(max_scale) = monitors
        .iter()
        .map(|m| m.scale_factor as f32)
        .reduce(f32::max)
    else {
        return;
    };
    if max_scale <= 0.0 {
        return;
    }

    let target_monitor_ref = match *target_monitor {
        WallpaperTargetMonitor::Primary => Some(*primary_monitor),
        WallpaperTargetMonitor::Index(n) => monitors.get(n).copied(),
        WallpaperTargetMonitor::All => None,
    };

    let (origin_x, origin_y, logical_width, logical_height) = if let Some(m) = target_monitor_ref {
        let width = ((m.physical_width as f32) / max_scale).ceil().max(1.0) as u32;
        let height = ((m.physical_height as f32) / max_scale).ceil().max(1.0) as u32;
        (m.physical_position.x, m.physical_position.y, width, height)
    } else {
        let width = ((max_x - min_x) as f32 / max_scale).ceil().max(1.0) as u32;
        let height = ((max_y - min_y) as f32 / max_scale).ceil().max(1.0) as u32;
        (min_x, min_y, width, height)
    };

    let logical_offset_x = ((origin_x - min_x) as f32 / max_scale).floor() as i32;
    let logical_offset_y = ((origin_y - min_y) as f32 / max_scale).floor() as i32;
    surface_info.set(
        logical_offset_x,
        logical_offset_y,
        logical_width,
        logical_height,
    );

    let Some((cursor_x, cursor_y)) = current_cursor_position() else {
        return;
    };

    let logical_position = Vec2::new(
        (cursor_x - min_x) as f32 / max_scale,
        (cursor_y - min_y) as f32 / max_scale,
    );

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

fn detect_last_button(
    prev: Option<&HashSet<MouseButton>>,
    current: &HashSet<MouseButton>,
) -> Option<PointerButton> {
    let empty = HashSet::new();
    let prev = prev.unwrap_or(&empty);

    let mut newly_pressed: Vec<MouseButton> = current.difference(prev).copied().collect();
    if let Some(btn) = prioritize_button(&mut newly_pressed) {
        return Some(PointerButton {
            button: Some(btn),
            pressed: true,
        });
    }

    let mut released: Vec<MouseButton> = prev.difference(current).copied().collect();
    if let Some(btn) = prioritize_button(&mut released) {
        return Some(PointerButton {
            button: Some(btn),
            pressed: false,
        });
    }

    None
}

fn prioritize_button(buttons: &mut Vec<MouseButton>) -> Option<MouseButton> {
    let priority = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];

    for p in priority {
        if let Some(pos) = buttons.iter().position(|b| *b == p) {
            return Some(buttons.swap_remove(pos));
        }
    }

    buttons.pop()
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
