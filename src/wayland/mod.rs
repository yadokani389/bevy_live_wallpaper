pub mod backend;
pub mod render;
pub mod surface;

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use wayland_client::Proxy;
use wayland_client::protocol::wl_display;
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{
        wl_callback, wl_compositor, wl_output, wl_pointer, wl_registry, wl_seat, wl_surface,
    },
};
use wayland_protocols::xdg::xdg_output::zv1::client::{zxdg_output_manager_v1, zxdg_output_v1};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use self::surface::WaylandSurfaceHandles;

#[derive(Clone, Debug)]
pub(crate) struct PointerFocus {
    output: u32,
    position: Vec2,
}

#[derive(Resource)]
pub(crate) struct WaylandAppState {
    pub closed: bool,
    pub pending_surface_config: Vec<WaylandSurfaceConfig>,
    /// Outputs whose geometry/scale changed since last frame.
    pub dirty_outputs: HashSet<u32>,
    pub pending_pointer_events: Vec<PendingPointerEvent>,
    pub pointer_focus: Option<PointerFocus>,
    // Wayland objects
    pub display: wl_display::WlDisplay,
    pub compositor: Option<(wl_compositor::WlCompositor, u32)>,
    pub layer_shell: Option<(zwlr_layer_shell_v1::ZwlrLayerShellV1, u32)>,
    pub seats: HashMap<u32, wl_seat::WlSeat>,
    pub pointers: HashMap<u32, wl_pointer::WlPointer>,
    pub outputs: HashMap<u32, wl_output::WlOutput>,
    pub output_info: HashMap<u32, OutputInfo>,
    pub output_order: Vec<u32>,
    pub surfaces: HashMap<u32, OutputSurface>,
    pub surface_to_output: HashMap<u32, u32>,
    pub xdg_output_manager: Option<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
    pub xdg_outputs: HashMap<u32, zxdg_output_v1::ZxdgOutputV1>,
}

pub(crate) struct OutputSurface {
    pub surface: wl_surface::WlSurface,
    pub layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingPointerEvent {
    output: u32,
    position: Vec2,
    offset: Vec2,
    kind: PendingPointerEventKind,
}

#[derive(Clone, Debug)]
pub(crate) enum PendingPointerEventKind {
    Motion,
    Button {
        button: Option<MouseButton>,
        pressed: bool,
    },
}

impl PendingPointerEventKind {
    /// Returns button state transition if this event represents a button action.
    fn button_change(&self) -> Option<(Option<MouseButton>, bool)> {
        match self {
            PendingPointerEventKind::Motion => None,
            PendingPointerEventKind::Button { button, pressed } => Some((*button, *pressed)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OutputInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: i32,
}

impl WaylandAppState {
    pub(crate) fn new(display: wl_display::WlDisplay) -> Self {
        Self {
            closed: false,
            pending_surface_config: Vec::new(),
            dirty_outputs: HashSet::new(),
            pending_pointer_events: Vec::new(),
            pointer_focus: None,
            display,
            compositor: None,
            layer_shell: None,
            seats: HashMap::new(),
            pointers: HashMap::new(),
            outputs: HashMap::new(),
            output_info: HashMap::new(),
            output_order: Vec::new(),
            surfaces: HashMap::new(),
            surface_to_output: HashMap::new(),
            xdg_output_manager: None,
            xdg_outputs: HashMap::new(),
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        !self.closed
    }

    pub(crate) fn queue_surface_config(&mut self, surface_state: WaylandSurfaceConfig) {
        self.pending_surface_config.push(surface_state);
    }

    pub(crate) fn take_surface_config(&mut self) -> Vec<WaylandSurfaceConfig> {
        std::mem::take(&mut self.pending_surface_config)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct WaylandSurfaceConfig {
    pub output: u32,
    pub handles: WaylandSurfaceHandles,
    pub width: u32,
    pub height: u32,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandAppState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                let _span_guard =
                    trace_span!("wl_registry::Event::Global", name, interface, version).entered();
                match interface.as_str() {
                    "wl_compositor" => {
                        info!("Compositor found: {} (version {})", name, version);
                        state.compositor = Some((registry.bind(name, version, qh, ()), name));
                    }
                    "wl_seat" => {
                        info!("Seat found: {} (version {})", name, version);
                        let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, version, qh, ());
                        state.seats.insert(name, seat);
                    }
                    "wl_output" => {
                        info!("Output found: {} (version {})", name, version);
                        let output =
                            registry.bind::<wl_output::WlOutput, _, _>(name, version, qh, ());
                        state.outputs.insert(name, output);
                        state.output_order.push(name);
                    }
                    "zwlr_layer_shell_v1" => {
                        info!("LayerShell found: {} (version {})", name, version);
                        state.layer_shell = Some((registry.bind(name, version, qh, ()), name));
                    }
                    "zxdg_output_manager_v1" => {
                        info!("xdg_output_manager found: {} (version {})", name, version);
                        state.xdg_output_manager = Some(registry.bind(name, version, qh, ()));
                    }
                    _ => {}
                }
            }
            wl_registry::Event::GlobalRemove { name } => {
                let _span_guard = trace_span!("wl_registry::Event::GlobalRemove", name).entered();
                if let Some((_, compositor_name)) = &state.compositor
                    && *compositor_name == name
                {
                    warn!("Compositor {} removed", name);
                    state.compositor = None;
                }
                if state.outputs.remove(&name).is_some() {
                    warn!("Output {} removed", name);
                    state.surfaces.remove(&name);
                    state.surface_to_output.retain(|_, output| *output != name);
                    state.output_order.retain(|n| *n != name);
                    if state
                        .pointer_focus
                        .as_ref()
                        .map(|focus| focus.output == name)
                        .unwrap_or(false)
                    {
                        state.pointer_focus = None;
                    }
                    if let Some(xdg) = state.xdg_outputs.remove(&name) {
                        xdg.destroy();
                    }
                }
                if let Some(seat) = state.seats.remove(&name) {
                    warn!("Seat {} removed", name);
                    let seat_id = seat.id().protocol_id();
                    if let Some(pointer) = state.pointers.remove(&seat_id) {
                        pointer.release();
                    }
                    seat.release();
                }
                if let Some((_, layer_shell_name)) = &state.layer_shell
                    && *layer_shell_name == name
                {
                    warn!("LayerShell {} removed", name);
                    state.layer_shell = None;
                }
            }
            _ => {}
        };
    }
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for WaylandAppState {
    fn event(
        _state: &mut Self,
        _layer_shell: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _event: zwlr_layer_shell_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Do nothing: LayerShell never dispatches events.
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WaylandAppState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_seat::Event::Capabilities { capabilities } => {
                let has_pointer = matches!(
                    capabilities,
                    wayland_client::WEnum::Value(cap)
                        if cap.contains(wl_seat::Capability::Pointer)
                );
                let seat_id = seat.id().protocol_id();

                if has_pointer {
                    state
                        .pointers
                        .entry(seat_id)
                        .or_insert_with(|| seat.get_pointer(qh, seat_id));
                } else if let Some(pointer) = state.pointers.remove(&seat_id) {
                    pointer.release();
                }
            }
            wl_seat::Event::Name { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, u32> for WaylandAppState {
    fn event(
        state: &mut Self,
        _pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _seat_id: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                let output = state
                    .surface_to_output
                    .get(&surface.id().protocol_id())
                    .copied()
                    .unwrap_or(u32::MAX);
                let offset = state
                    .output_info
                    .get(&output)
                    .map(|info| Vec2::new(info.x as f32, info.y as f32))
                    .unwrap_or(Vec2::ZERO);
                let position = Vec2::new(surface_x as f32, surface_y as f32);
                state.pointer_focus = Some(PointerFocus { output, position });
                state.pending_pointer_events.push(PendingPointerEvent {
                    output,
                    position,
                    offset,
                    kind: PendingPointerEventKind::Motion,
                });
            }
            wl_pointer::Event::Leave { .. } => {
                state.pointer_focus = None;
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                if let Some(focus) = state.pointer_focus.as_mut() {
                    let offset = state
                        .output_info
                        .get(&focus.output)
                        .map(|info| Vec2::new(info.x as f32, info.y as f32))
                        .unwrap_or(Vec2::ZERO);
                    focus.position = Vec2::new(surface_x as f32, surface_y as f32);
                    state.pending_pointer_events.push(PendingPointerEvent {
                        output: focus.output,
                        position: focus.position,
                        offset,
                        kind: PendingPointerEventKind::Motion,
                    });
                }
            }
            wl_pointer::Event::Button {
                button,
                state: btn_state,
                ..
            } => {
                if let Some(focus) = state.pointer_focus.as_ref() {
                    let offset = state
                        .output_info
                        .get(&focus.output)
                        .map(|info| Vec2::new(info.x as f32, info.y as f32))
                        .unwrap_or(Vec2::ZERO);

                    let map_pointer_button = |code: u32| -> Option<MouseButton> {
                        match code {
                            272 => Some(MouseButton::Left),
                            273 => Some(MouseButton::Right),
                            274 => Some(MouseButton::Middle),
                            other => u16::try_from(other).ok().map(MouseButton::Other),
                        }
                    };

                    state.pending_pointer_events.push(PendingPointerEvent {
                        output: focus.output,
                        position: focus.position,
                        offset,
                        kind: PendingPointerEventKind::Button {
                            button: map_pointer_button(button),
                            pressed: matches!(
                                btn_state,
                                wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed)
                            ),
                        },
                    });
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for WaylandAppState {
    fn event(
        state: &mut Self,
        surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                let _span_guard = trace_span!(
                    "zwlr_layer_surface_v1::Event::Configure",
                    serial,
                    width,
                    height
                );
                info!(
                    "Layer surface configured: serial={}, width={}, height={}",
                    serial, width, height
                );
                surface.ack_configure(serial);
                if let Some((output, surf)) = state
                    .surfaces
                    .iter()
                    .find(|(_, entry)| entry.layer_surface == *surface)
                {
                    // bind xdg_output if available and not yet bound
                    if let (Some(manager), Some(wl_output)) =
                        (state.xdg_output_manager.as_ref(), state.outputs.get(output))
                    {
                        state
                            .xdg_outputs
                            .entry(*output)
                            .or_insert_with(|| manager.get_xdg_output(wl_output, qh, *output));
                    }

                    let handles = WaylandSurfaceHandles::new(&state.display, &surf.surface);
                    let width = width.max(1);
                    let height = height.max(1);
                    let (offset_x, offset_y) = state
                        .output_info
                        .get(output)
                        .map(|i| (i.x, i.y))
                        .unwrap_or((0, 0));
                    state.queue_surface_config(WaylandSurfaceConfig {
                        output: *output,
                        handles,
                        width,
                        height,
                        offset_x,
                        offset_y,
                    });
                } else {
                    warn!("Configure for unknown layer_surface");
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                let _span_guard = trace_span!("zwlr_layer_surface_v1::Event::Closed").entered();
                info!("Layer surface closed");
                // Mark closed; will be cleaned up by event loop.
                state.closed = true;
            }
            _ => (),
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for WaylandAppState {
    fn event(
        _state: &mut Self,
        _callback: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_callback::Event::Done { .. } => {
                let _span_guard = trace_span!("wl_callback::Event::Done").entered();
                // Frame callback done, can be used to trigger next render
                trace!("Frame callback received");
            }
            _ => {
                // Do nothing
            }
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for WaylandAppState {
    fn event(
        _state: &mut Self,
        _surface: &wl_surface::WlSurface,
        event: wl_surface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_surface::Event::Enter { .. } => {
                // Do nothing: Cursor enter event is not needed for background.
            }
            wl_surface::Event::Leave { .. } => {
                // Do nothing: Cursor leave event is not needed for background.
            }
            wl_surface::Event::PreferredBufferScale { factor } => {
                debug!("Preferred buffer scale factor: {}", factor);
            }
            wl_surface::Event::PreferredBufferTransform { transform } => {
                // todo: Device rotation support
                debug!("TODO: Handle preferred buffer transform: {:?}", transform);
            }
            _ => {
                // Do nothing
            }
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for WaylandAppState {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Geometry { x, y, .. } => {
                let info = state
                    .output_info
                    .entry(output.id().protocol_id())
                    .or_default();
                info.x = x;
                info.y = y;
                state.dirty_outputs.insert(output.id().protocol_id());
            }
            wl_output::Event::Mode { width, height, .. } => {
                let info = state
                    .output_info
                    .entry(output.id().protocol_id())
                    .or_default();
                info.width = width;
                info.height = height;
                state.dirty_outputs.insert(output.id().protocol_id());
            }
            wl_output::Event::Scale { factor } => {
                let info = state
                    .output_info
                    .entry(output.id().protocol_id())
                    .or_default();
                info.scale = factor;
                state.dirty_outputs.insert(output.id().protocol_id());
            }
            _ => {}
        }
    }
}

impl Dispatch<zxdg_output_manager_v1::ZxdgOutputManagerV1, ()> for WaylandAppState {
    fn event(
        _state: &mut Self,
        _object: &zxdg_output_manager_v1::ZxdgOutputManagerV1,
        _event: zxdg_output_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // manager has no events
    }
}

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, u32> for WaylandAppState {
    fn event(
        state: &mut Self,
        _output: &zxdg_output_v1::ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        output_name: &u32,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                let info = state.output_info.entry(*output_name).or_default();
                info.x = x;
                info.y = y;
                state.dirty_outputs.insert(*output_name);
            }
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                let info = state.output_info.entry(*output_name).or_default();
                info.width = width;
                info.height = height;
                state.dirty_outputs.insert(*output_name);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for WaylandAppState {
    fn event(
        _state: &mut Self,
        _compositor: &wl_compositor::WlCompositor,
        _event: wl_compositor::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Do nothing: Compositor never dispatches events.
    }
}
