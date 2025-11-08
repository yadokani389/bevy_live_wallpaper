use std::ffi::c_void;
use std::ptr::NonNull;

use wayland_client::Proxy;
use wayland_client::protocol::{wl_display::WlDisplay, wl_surface::WlSurface};
use wgpu::rwh::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaylandSurfaceHandles {
    display_ptr: usize,
    window_ptr: usize,
}

impl WaylandSurfaceHandles {
    pub fn new(display: &WlDisplay, surface: &WlSurface) -> Self {
        Self {
            display_ptr: display.id().as_ptr() as usize,
            window_ptr: surface.id().as_ptr() as usize,
        }
    }

    pub fn raw_display_handle(&self) -> RawDisplayHandle {
        let handle = WaylandDisplayHandle::new(
            NonNull::new(self.display_ptr as *mut c_void).expect("display ptr should be valid"),
        );
        RawDisplayHandle::Wayland(handle)
    }

    pub fn raw_window_handle(&self) -> RawWindowHandle {
        let handle = WaylandWindowHandle::new(
            NonNull::new(self.window_ptr as *mut c_void).expect("window ptr should be valid"),
        );
        RawWindowHandle::Wayland(handle)
    }
}
