use bevy::prelude::*;
use bevy::window::{Monitor, PrimaryMonitor, RawHandleWrapper};
use raw_window_handle::RawWindowHandle;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, FindWindowExW, FindWindowW, GWL_EXSTYLE, GWL_STYLE,
    GetClassNameW, GetWindowLongW, PostMessageW, SEND_MESSAGE_TIMEOUT_FLAGS, SendMessageTimeoutW,
    SetParent, SetWindowLongW, WM_CLOSE, WS_CHILD, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_OVERLAPPED, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR};

#[derive(Default)]
pub struct WallpaperWindowsPlugin {
    pub target_monitor: WallpaperTargetMonitor,
}

impl Plugin for WallpaperWindowsPlugin {
    fn build(&self, app: &mut App) {
        let workerw = find_workerw().expect("workerw not found.");
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
