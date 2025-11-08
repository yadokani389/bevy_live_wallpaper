pub mod backend;
pub mod render;
pub mod surface;

use bevy::prelude::*;
use wayland_client::protocol::wl_display;
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::{wl_callback, wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use self::surface::WaylandSurfaceHandles;

/// Marks a camera whose output should be redirected to the Wayland background surface.
/// This component is only used on Wayland.
#[derive(Component, Default)]
pub struct LiveWallpaperCamera;

#[derive(Resource)]
pub struct WaylandAppState {
    pub closed: bool,
    pub pending_surface_config: Option<WaylandSurfaceConfig>,
    // Wayland objects
    pub display: wl_display::WlDisplay,
    pub compositor: Option<(wl_compositor::WlCompositor, u32)>,
    pub layer_shell: Option<(zwlr_layer_shell_v1::ZwlrLayerShellV1, u32)>,
    pub surface: Option<wl_surface::WlSurface>,
    pub layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
}

impl WaylandAppState {
    pub fn new(display: wl_display::WlDisplay) -> Self {
        Self {
            closed: false,
            pending_surface_config: None,
            display,
            compositor: None,
            layer_shell: None,
            surface: None,
            layer_surface: None,
        }
    }

    pub fn is_running(&self) -> bool {
        !self.closed
    }

    pub fn queue_surface_config(&mut self, surface_state: WaylandSurfaceConfig) {
        self.pending_surface_config = Some(surface_state);
    }

    pub fn take_surface_config(&mut self) -> Option<WaylandSurfaceConfig> {
        self.pending_surface_config.take()
    }
}

#[derive(Clone, Copy)]
pub struct WaylandSurfaceConfig {
    pub handles: WaylandSurfaceHandles,
    pub width: u32,
    pub height: u32,
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
                    "zwlr_layer_shell_v1" => {
                        info!("LayerShell found: {} (version {})", name, version);
                        state.layer_shell = Some((registry.bind(name, version, qh, ()), name));
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

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for WaylandAppState {
    fn event(
        state: &mut Self,
        surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
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
                if let Some(surface) = state.surface.as_ref().cloned() {
                    let handles = WaylandSurfaceHandles::new(&state.display, &surface);
                    let width = width.max(1);
                    let height = height.max(1);
                    state.queue_surface_config(WaylandSurfaceConfig {
                        handles,
                        width,
                        height,
                    });
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                let _span_guard = trace_span!("zwlr_layer_surface_v1::Event::Closed").entered();
                info!("Layer surface closed");
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
