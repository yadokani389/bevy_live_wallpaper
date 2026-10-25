use bevy::{
    camera::CameraUpdateSystems,
    prelude::*,
    render::{Render, RenderApp, RenderSystems, extract_resource::ExtractResourcePlugin},
};

use crate::{
    WallpaperPointerState, WallpaperSurfaceInfo, WallpaperTargetMonitor,
    render_target::{
        WallpaperRenderTarget, WallpaperRenderTargetPlugin, assign_wallpaper_camera_target,
    },
};

use super::{
    X11AppState,
    render::{X11GpuSurfaceState, X11SurfaceDescriptor, prepare_x11_surface, present_x11_surface},
};

#[derive(Default)]
pub(crate) struct X11BackendPlugin;

impl Plugin for X11BackendPlugin {
    fn build(&self, app: &mut App) {
        let target_monitor = app
            .world()
            .get_resource::<WallpaperTargetMonitor>()
            .copied()
            .unwrap_or_default();

        let (app_state, initial_config) = X11AppState::connect(target_monitor)
            .expect("failed to initialize X11 wallpaper backend");

        info!(
            "Connected to X11 wallpaper window: {}x{} (target: {:?})",
            initial_config.width, initial_config.height, target_monitor
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
            .add_systems(
                Render,
                present_x11_surface
                    .in_set(RenderSystems::Cleanup)
                    .after(RenderSystems::Render),
            );

        app.init_resource::<X11SurfaceDescriptor>()
            .add_plugins((
                ExtractResourcePlugin::<X11SurfaceDescriptor>::default(),
                WallpaperRenderTargetPlugin,
            ))
            .add_systems(PostUpdate, x11_event_system)
            .add_systems(
                PostUpdate,
                (
                    sync_x11_render_target_image.after(x11_event_system),
                    assign_wallpaper_camera_target.after(sync_x11_render_target_image),
                )
                    .before(CameraUpdateSystems),
            )
            .insert_non_send(app_state);
    }
}

fn x11_event_system(
    mut app_state: NonSendMut<X11AppState>,
    mut surface_descriptor: ResMut<X11SurfaceDescriptor>,
    target_monitor: Res<WallpaperTargetMonitor>,
    mut pointer_state: ResMut<WallpaperPointerState>,
    mut surface_info: ResMut<WallpaperSurfaceInfo>,
) {
    if !app_state.is_running() {
        return;
    }

    app_state.poll_events();

    if target_monitor.is_changed()
        && let Err(err) = app_state.apply_target(*target_monitor)
    {
        warn!("Failed to apply target monitor change: {err}");
    }

    if let Some((x, y, w, h)) = app_state.current_bounds() {
        surface_info.set(x, y, w, h);
    }

    if let Some(sample) = app_state.poll_pointer(pointer_state.last.as_ref()) {
        pointer_state.last = Some(sample);
    }

    if let Some(surface_config) = app_state.take_surface_config() {
        info!(
            "X11 surface configured: {}x{}",
            surface_config.width, surface_config.height
        );
        surface_descriptor.handles = Some(surface_config.handles);
        surface_descriptor.width = surface_config.width;
        surface_descriptor.height = surface_config.height;
    }
}

fn sync_x11_render_target_image(
    descriptor: Res<X11SurfaceDescriptor>,
    target: Res<WallpaperRenderTarget>,
    mut images: ResMut<Assets<Image>>,
) {
    target.resize(&mut images, descriptor.width, descriptor.height);
}
