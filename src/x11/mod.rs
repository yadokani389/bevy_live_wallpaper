pub mod backend;
pub mod render;
pub mod surface;

use std::{
    collections::HashSet,
    ffi::{c_int, c_void},
    ptr::NonNull,
};

use as_raw_xcb_connection::AsRawXcbConnection;
use bevy::prelude::*;
use x11rb::COPY_DEPTH_FROM_PARENT;
use x11rb::protocol::randr::{self, ConnectionExt as RandrConnectionExt, MonitorInfo};
use x11rb::{
    connection::Connection,
    protocol::{
        Event,
        xproto::{ChangeWindowAttributesAux, ConnectionExt, EventMask},
    },
    xcb_ffi::XCBConnection,
};

use self::surface::X11SurfaceHandles;

use crate::{PointerButton, PointerSample, WallpaperTargetMonitor};

pub(crate) struct X11AppState {
    connection: XCBConnection,
    root_window: u32,
    wallpaper_window: u32,
    screen: c_int,
    closed: bool,
    target: WallpaperTargetMonitor,
    monitors: Vec<MonitorRect>,
    monitors_dirty: bool,
    pending_surface_config: Option<X11SurfaceConfig>,
}

impl X11AppState {
    pub(crate) fn connect(
        target: WallpaperTargetMonitor,
    ) -> Result<(Self, X11SurfaceConfig), String> {
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
        let root_visual = screen.root_visual;
        let screen_id = screen_index as c_int;

        connection
            .change_window_attributes(
                root_window,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::STRUCTURE_NOTIFY),
            )
            .map_err(|err| format!("Failed to select root window events: {err:?}"))?
            .check()
            .map_err(|err| format!("Failed to select root window events: {err:?}"))?;

        // Subscribe to RandR notifications (monitor hotplug/resize).
        connection
            .randr_select_input(
                root_window,
                randr::NotifyMask::CRTC_CHANGE
                    | randr::NotifyMask::OUTPUT_CHANGE
                    | randr::NotifyMask::SCREEN_CHANGE,
            )
            .map_err(|err| format!("Failed to select RandR input: {err:?}"))?;

        connection
            .flush()
            .map_err(|err| format!("Failed to flush X11 connection: {err:?}"))?;

        let mut state = Self {
            connection,
            root_window,
            wallpaper_window: 0,
            screen: screen_id,
            closed: false,
            target,
            monitors: Vec::new(),
            monitors_dirty: true,
            pending_surface_config: None,
        };

        state.refresh_monitors()?;
        state.create_or_update_wallpaper_window(root_visual)?;
        state.monitors_dirty = false;

        // Initial surface config uses current wallpaper window size.
        let config = Self::create_surface_config(
            &state.connection,
            state.wallpaper_window,
            screen_id,
            state.current_width().unwrap_or(screen_width),
            state.current_height().unwrap_or(screen_height),
        );

        state.pending_surface_config = Some(config);

        Ok((state, config))
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

    fn current_width(&self) -> Option<u32> {
        self.monitor_for(self.target).map(|rect| rect.width as u32)
    }

    fn current_height(&self) -> Option<u32> {
        self.monitor_for(self.target).map(|rect| rect.height as u32)
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
                    if event.window == self.wallpaper_window {
                        let width = u32::from(event.width.max(1));
                        let height = u32::from(event.height.max(1));
                        let config = Self::create_surface_config(
                            &self.connection,
                            self.wallpaper_window,
                            self.screen,
                            width,
                            height,
                        );
                        self.queue_surface_config(config);
                    }
                }
                Ok(Some(Event::RandrNotify(_))) | Ok(Some(Event::RandrScreenChangeNotify(_))) => {
                    self.monitors_dirty = true;
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

        if self.monitors_dirty && !self.closed {
            if let Err(err) = self.refresh_monitors() {
                warn!("Failed to refresh RandR monitors: {err}");
            } else if let Err(err) = self.apply_target(self.target) {
                warn!("Failed to apply target monitor after RandR change: {err}");
            }
            self.monitors_dirty = false;
        }
    }

    /// Returns a snapshot of the current pointer (root) position and buttons.
    pub(crate) fn poll_pointer(&self, prev: Option<&PointerSample>) -> Option<PointerSample> {
        let reply = self
            .connection
            .query_pointer(self.root_window)
            .ok()?
            .reply()
            .ok()?;

        let position = Vec2::new(f32::from(reply.root_x), f32::from(reply.root_y));
        let prev_position = prev.map(|p| p.position).unwrap_or(position);
        let delta = position - prev_position;

        let pressed = pressed_buttons(reply.mask.bits());
        let last_button = detect_last_button(prev.map(|p| &p.pressed), &pressed);

        let output = self.output_for_position(position);

        Some(PointerSample {
            output,
            position,
            delta,
            pressed,
            last_button,
        })
    }

    fn output_for_position(&self, position: Vec2) -> Option<u32> {
        self.monitors
            .iter()
            .enumerate()
            .find(|(_, rect)| {
                let px = position.x as i32;
                let py = position.y as i32;
                px >= rect.x as i32
                    && px < rect.x as i32 + rect.width as i32
                    && py >= rect.y as i32
                    && py < rect.y as i32 + rect.height as i32
            })
            .map(|(idx, _)| idx as u32)
    }

    pub(crate) fn apply_target(&mut self, target: WallpaperTargetMonitor) -> Result<(), String> {
        let Some(rect) = self.monitor_for(target) else {
            return Err("No monitors available for selected target".into());
        };

        self.target = target;

        // Move/resize wallpaper window to selected monitor bounds.
        let aux = x11rb::protocol::xproto::ConfigureWindowAux::new()
            .x(i32::from(rect.x))
            .y(i32::from(rect.y))
            .width(rect.width as u32)
            .height(rect.height as u32)
            .stack_mode(x11rb::protocol::xproto::StackMode::BELOW);
        self.connection
            .configure_window(self.wallpaper_window, &aux)
            .map_err(|err| format!("Failed to configure wallpaper window: {err:?}"))?
            .check()
            .map_err(|err| format!("Failed to configure wallpaper window: {err:?}"))?;
        self.connection
            .flush()
            .map_err(|err| format!("Failed to flush wallpaper configure: {err:?}"))?;

        let config = Self::create_surface_config(
            &self.connection,
            self.wallpaper_window,
            self.screen,
            rect.width as u32,
            rect.height as u32,
        );
        self.queue_surface_config(config);
        Ok(())
    }

    fn refresh_monitors(&mut self) -> Result<(), String> {
        let reply = self
            .connection
            .randr_get_monitors(self.root_window, true)
            .map_err(|err| format!("Failed to request RandR monitors: {err:?}"))?
            .reply()
            .map_err(|err| format!("Failed to read RandR monitors reply: {err:?}"))?;

        self.monitors = reply.monitors.into_iter().map(MonitorRect::from).collect();
        Ok(())
    }

    fn monitor_for(&self, target: WallpaperTargetMonitor) -> Option<MonitorRect> {
        match target {
            WallpaperTargetMonitor::All => MonitorRect::bounding(&self.monitors),
            WallpaperTargetMonitor::Primary => self
                .monitors
                .iter()
                .find(|m| m.primary)
                .or_else(|| self.monitors.first())
                .copied(),
            WallpaperTargetMonitor::Index(n) => self.monitors.get(n).copied(),
        }
    }

    pub(crate) fn current_bounds(&self) -> Option<(i32, i32, u32, u32)> {
        self.monitor_for(self.target).map(|rect| {
            (
                rect.x as i32,
                rect.y as i32,
                rect.width as u32,
                rect.height as u32,
            )
        })
    }

    fn create_or_update_wallpaper_window(&mut self, visual: u32) -> Result<(), String> {
        if self.monitors.is_empty() {
            return Err("No monitors reported by RandR; cannot create wallpaper window".into());
        }

        let rect = self
            .monitor_for(self.target)
            .unwrap_or_else(|| self.monitors[0]);

        if self.wallpaper_window == 0 {
            let window = self
                .connection
                .generate_id()
                .map_err(|err| format!("Failed to generate window id: {err:?}"))?;

            let aux = x11rb::protocol::xproto::CreateWindowAux::new()
                .event_mask(EventMask::STRUCTURE_NOTIFY)
                .override_redirect(1)
                .background_pixel(0)
                .border_pixel(0);

            self.connection
                .create_window(
                    COPY_DEPTH_FROM_PARENT,
                    window,
                    self.root_window,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    0,
                    x11rb::protocol::xproto::WindowClass::INPUT_OUTPUT,
                    visual,
                    &aux,
                )
                .map_err(|err| format!("Failed to create wallpaper window: {err:?}"))?
                .check()
                .map_err(|err| format!("Failed to create wallpaper window: {err:?}"))?;

            // Place behind other windows.
            let config_aux = x11rb::protocol::xproto::ConfigureWindowAux::new()
                .stack_mode(x11rb::protocol::xproto::StackMode::BELOW);
            self.connection
                .configure_window(window, &config_aux)
                .map_err(|err| format!("Failed to lower wallpaper window: {err:?}"))?
                .check()
                .map_err(|err| format!("Failed to lower wallpaper window: {err:?}"))?;

            self.connection
                .map_window(window)
                .map_err(|err| format!("Failed to map wallpaper window: {err:?}"))?
                .check()
                .map_err(|err| format!("Failed to map wallpaper window: {err:?}"))?;

            self.wallpaper_window = window;
        } else {
            self.apply_target(self.target)?;
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct X11SurfaceConfig {
    pub handles: X11SurfaceHandles,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Default)]
struct MonitorRect {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    primary: bool,
}

impl MonitorRect {
    fn bounding(monitors: &[Self]) -> Option<Self> {
        let mut iter = monitors.iter();
        let first = iter.next().copied()?;

        let mut min_x = first.x as i32;
        let mut min_y = first.y as i32;
        let mut max_x = first.x as i32 + first.width as i32;
        let mut max_y = first.y as i32 + first.height as i32;

        for m in iter {
            min_x = min_x.min(m.x as i32);
            min_y = min_y.min(m.y as i32);
            max_x = max_x.max(m.x as i32 + m.width as i32);
            max_y = max_y.max(m.y as i32 + m.height as i32);
        }

        Some(Self {
            x: min_x as i16,
            y: min_y as i16,
            width: (max_x - min_x) as u16,
            height: (max_y - min_y) as u16,
            primary: false,
        })
    }
}

impl From<MonitorInfo> for MonitorRect {
    fn from(m: MonitorInfo) -> Self {
        Self {
            x: m.x,
            y: m.y,
            width: m.width,
            height: m.height,
            primary: m.primary,
        }
    }
}

fn pressed_buttons(mask: u16) -> HashSet<MouseButton> {
    let mut set = HashSet::new();

    let has = |button: u8| -> bool { mask & (1u16 << (button + 7)) != 0 };

    if has(1) {
        set.insert(MouseButton::Left);
    }
    if has(2) {
        set.insert(MouseButton::Middle);
    }
    if has(3) {
        set.insert(MouseButton::Right);
    }

    // Ignore BUTTON_4/BUTTON_5 (scroll) to avoid treating wheel motion as held buttons.

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
    // Deterministic priority similar to common UX expectations.
    let priority = [MouseButton::Left, MouseButton::Right, MouseButton::Middle];

    for p in priority {
        if let Some(pos) = buttons.iter().position(|b| *b == p) {
            return Some(buttons.swap_remove(pos));
        }
    }

    buttons.pop()
}
