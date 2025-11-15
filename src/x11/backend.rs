use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems, extract_resource::ExtractResourcePlugin,
        render_resource::Extent3d,
    },
};

use crate::LiveWallpaperCamera;

use super::{
    X11AppState,
    render::{
        X11GpuSurfaceState, X11RenderTarget, X11SurfaceDescriptor, create_x11_image,
        prepare_x11_surface, present_x11_surface,
    },
};

#[derive(Default)]
pub(crate) struct X11BackendPlugin;

impl Plugin for X11BackendPlugin {
    fn build(&self, app: &mut App) {
        let (app_state, initial_config) =
            X11AppState::connect().expect("failed to initialize X11 wallpaper backend");

        info!(
            "Connected to X11 root window: {}x{}",
            initial_config.width, initial_config.height
        );

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .init_resource::<X11GpuSurfaceState>()
            .add_systems(
                Render,
                prepare_x11_surface.in_set(RenderSystems::PrepareResources),
            )
            .add_systems(Render, present_x11_surface.in_set(RenderSystems::Cleanup));

        let target_image = {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            create_x11_image(&mut images)
        };

        app.insert_resource(X11SurfaceDescriptor::new())
            .insert_resource(X11RenderTarget::new(target_image))
            .add_plugins((
                ExtractResourcePlugin::<X11SurfaceDescriptor>::default(),
                ExtractResourcePlugin::<X11RenderTarget>::default(),
            ))
            .add_systems(PostUpdate, x11_event_system)
            .add_systems(
                PostUpdate,
                (
                    sync_x11_render_target_image.after(x11_event_system),
                    assign_x11_camera_target.after(sync_x11_render_target_image),
                ),
            )
            .insert_non_send_resource(app_state);
    }
}

fn x11_event_system(
    mut app_state: NonSendMut<X11AppState>,
    mut surface_descriptor: ResMut<X11SurfaceDescriptor>,
) {
    if !app_state.is_running() {
        return;
    }

    app_state.poll_events();

    if let Some(surface_config) = app_state.take_surface_config() {
        info!(
            "X11 surface configured: {}x{}",
            surface_config.width, surface_config.height
        );
        surface_descriptor.handles = Some(surface_config.handles);
        surface_descriptor.width = surface_config.width;
        surface_descriptor.height = surface_config.height;
        surface_descriptor.bump_generation();
    }
}

fn sync_x11_render_target_image(
    descriptor: Res<X11SurfaceDescriptor>,
    mut target: ResMut<X11RenderTarget>,
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

fn assign_x11_camera_target(
    target: Res<X11RenderTarget>,
    mut cameras: Query<&mut Camera, With<LiveWallpaperCamera>>,
) {
    for mut camera in &mut cameras {
        camera.target = RenderTarget::Image(target.image.clone().into());
    }
}
