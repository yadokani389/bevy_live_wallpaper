pub mod backend;
pub mod render;
pub mod surface;

use std::{
    ffi::{c_int, c_void},
    ptr::NonNull,
};

use as_raw_xcb_connection::AsRawXcbConnection;
use bevy::prelude::*;
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{ChangeWindowAttributesAux, ConnectionExt, EventMask},
    },
    xcb_ffi::XCBConnection,
};

use self::surface::X11SurfaceHandles;

pub(crate) struct X11AppState {
    connection: XCBConnection,
    root_window: u32,
    screen: c_int,
    closed: bool,
    pending_surface_config: Option<X11SurfaceConfig>,
}

impl X11AppState {
    pub(crate) fn connect() -> Result<(Self, X11SurfaceConfig), String> {
        let (connection, screen_index) = XCBConnection::connect(None)
            .map_err(|err| format!("Failed to connect to X11: {err}"))?;

        let screen = connection
            .setup()
            .roots
            .get(screen_index)
            .ok_or_else(|| format!("Invalid X11 screen index {screen_index}"))?;
        let root_window = screen.root;
        let screen_width = u32::from(screen.width_in_pixels);
        let screen_height = u32::from(screen.height_in_pixels);
        let screen_id = screen_index as c_int;

        connection
            .change_window_attributes(
                root_window,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::STRUCTURE_NOTIFY),
            )
            .map_err(|err| format!("Failed to select root window events: {err:?}"))?
            .check()
            .map_err(|err| format!("Failed to select root window events: {err:?}"))?;

        connection
            .flush()
            .map_err(|err| format!("Failed to flush X11 connection: {err:?}"))?;

        let config = Self::create_surface_config(
            &connection,
            root_window,
            screen_id,
            screen_width,
            screen_height,
        );

        Ok((
            Self {
                connection,
                root_window,
                screen: screen_id,
                closed: false,
                pending_surface_config: Some(config),
            },
            config,
        ))
    }

    fn create_surface_config(
        connection: &XCBConnection,
        window: u32,
        screen: c_int,
        width: u32,
        height: u32,
    ) -> X11SurfaceConfig {
        let ptr = NonNull::new(connection.as_raw_xcb_connection().cast::<c_void>())
            .expect("xcb connection pointer should be valid");
        let handles = X11SurfaceHandles::new(ptr, screen, window);

        X11SurfaceConfig {
            handles,
            width,
            height,
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        !self.closed
    }

    pub(crate) fn queue_surface_config(&mut self, config: X11SurfaceConfig) {
        self.pending_surface_config = Some(config);
    }

    pub(crate) fn take_surface_config(&mut self) -> Option<X11SurfaceConfig> {
        self.pending_surface_config.take()
    }

    pub(crate) fn poll_events(&mut self) {
        loop {
            match self.connection.poll_for_event() {
                Ok(Some(Event::ConfigureNotify(event))) => {
                    if event.window == self.root_window {
                        let width = u32::from(event.width.max(1));
                        let height = u32::from(event.height.max(1));
                        let config = Self::create_surface_config(
                            &self.connection,
                            self.root_window,
                            self.screen,
                            width,
                            height,
                        );
                        self.queue_surface_config(config);
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(err) => {
                    warn!("X11 poll_for_event failed: {err:?}");
                    self.closed = true;
                    break;
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct X11SurfaceConfig {
    pub handles: X11SurfaceHandles,
    pub width: u32,
    pub height: u32,
}
