use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems, extract_resource::ExtractResourcePlugin,
        render_resource::Extent3d,
    },
};
use wayland_client::{Connection, EventQueue};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use super::LiveWallpaperCamera;

use super::{
    WaylandAppState,
    render::{
        WaylandGpuSurfaceState, WaylandRenderTarget, WaylandSurfaceDescriptor,
        create_wayland_image, prepare_wayland_surface, present_wayland_surface,
    },
};

#[derive(Default)]
pub(crate) struct WaylandBackendPlugin;

impl Plugin for WaylandBackendPlugin {
    fn build(&self, app: &mut App) {
        let conn = Connection::connect_to_env().unwrap();
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();

        let display = conn.display();
        display.get_registry(&qh, ());

        let mut app_state = WaylandAppState::new(display.clone());

        info!("Waiting for globals...");
        event_queue.roundtrip(&mut app_state).unwrap();
        info!("Globals received.");

        let compositor = app_state.compositor.as_ref().expect("Compositor not found");
        let surface = compositor.0.create_surface(&qh, ());
        app_state.surface = Some(surface.clone());

        let layer_shell = app_state
            .layer_shell
            .as_ref()
            .expect("Layer shell not found");
        let layer_surface = layer_shell.0.get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Bottom,
            "egl_background".to_string(),
            &qh,
            (),
        );
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_anchor(
            zwlr_layer_surface_v1::Anchor::Top
                | zwlr_layer_surface_v1::Anchor::Bottom
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );
        layer_surface.set_size(0, 0);
        app_state.layer_surface = Some(layer_surface);

        surface.commit();
        info!("Initial commit done. Waiting for configure event...");

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<WaylandGpuSurfaceState>()
            .add_systems(
                Render,
                prepare_wayland_surface.in_set(RenderSystems::PrepareResources),
            )
            .add_systems(
                Render,
                present_wayland_surface.in_set(RenderSystems::Cleanup),
            );

        let target_image = {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            create_wayland_image(&mut images)
        };

        app.insert_resource(WaylandSurfaceDescriptor::new())
            .insert_resource(WaylandRenderTarget::new(target_image))
            .add_plugins((
                ExtractResourcePlugin::<WaylandSurfaceDescriptor>::default(),
                ExtractResourcePlugin::<WaylandRenderTarget>::default(),
            ))
            .add_systems(PostUpdate, wayland_event_system)
            .add_systems(
                PostUpdate,
                (
                    sync_wayland_render_target_image.after(wayland_event_system),
                    assign_wayland_camera_target.after(sync_wayland_render_target_image),
                ),
            )
            .insert_non_send_resource(WaylandEventQueue(event_queue))
            .insert_non_send_resource(app_state);
    }
}

#[derive(Resource, Deref, DerefMut)]
struct WaylandEventQueue(EventQueue<WaylandAppState>);

fn wayland_event_system(
    mut event_queue: NonSendMut<WaylandEventQueue>,
    mut app_state: NonSendMut<WaylandAppState>,
    mut surface_descriptor: ResMut<WaylandSurfaceDescriptor>,
) {
    if app_state.is_running() {
        if let Err(err) = event_queue.blocking_dispatch(&mut app_state) {
            warn!("Wayland event dispatch failed: {err:?}; closing background surface");
            app_state.closed = true;
            surface_descriptor.handles = None;
            surface_descriptor.bump_generation();
            return;
        }

        let mut touched = false;

        if let Some(surface_config) = app_state.take_surface_config() {
            info!(
                "Wayland surface configured: {}x{}",
                surface_config.width, surface_config.height
            );
            surface_descriptor.handles = Some(surface_config.handles);
            surface_descriptor.width = surface_config.width;
            surface_descriptor.height = surface_config.height;
            touched = true;
        }

        if touched {
            surface_descriptor.bump_generation();
        }
    }
}

fn sync_wayland_render_target_image(
    descriptor: Res<WaylandSurfaceDescriptor>,
    mut target: ResMut<WaylandRenderTarget>,
    mut images: ResMut<Assets<Image>>,
) {
    if descriptor.width == 0 || descriptor.height == 0 {
        return;
    }

    if target.last_applied_generation == descriptor.generation {
        return;
    }

    if let Some(image) = images.get_mut(&target.image) {
        let size = Extent3d {
            width: descriptor.width,
            height: descriptor.height,
            depth_or_array_layers: 1,
        };

        if image.texture_descriptor.size != size {
            image.texture_descriptor.size = size;
        }

        image.resize(size);
    }

    target.last_applied_generation = descriptor.generation;
}

fn assign_wayland_camera_target(
    target: Res<WaylandRenderTarget>,
    mut cameras: Query<&mut Camera, With<LiveWallpaperCamera>>,
) {
    for mut camera in &mut cameras {
        camera.target = RenderTarget::Image(target.image.clone().into());
    }
}
