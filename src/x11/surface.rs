use std::{ffi::c_void, num::NonZeroU32, ptr::NonNull};

use raw_window_handle::{RawDisplayHandle, RawWindowHandle, XcbDisplayHandle, XcbWindowHandle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct X11SurfaceHandles {
    connection_ptr: usize,
    window: u32,
    screen: i32,
}

impl X11SurfaceHandles {
    pub fn new(connection: NonNull<c_void>, screen: i32, window: u32) -> Self {
        Self {
            connection_ptr: connection.as_ptr() as usize,
            window,
            screen,
        }
    }

    pub fn raw_display_handle(&self) -> RawDisplayHandle {
        let connection = NonNull::new(self.connection_ptr as *mut c_void)
            .expect("connection pointer should remain valid");
        RawDisplayHandle::Xcb(XcbDisplayHandle::new(Some(connection), self.screen))
    }

    pub fn raw_window_handle(&self) -> RawWindowHandle {
        let window = NonZeroU32::new(self.window).expect("window id should not be zero");
        RawWindowHandle::Xcb(XcbWindowHandle::new(window))
    }
}
