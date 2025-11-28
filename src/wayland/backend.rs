use bevy::{
    camera::RenderTarget,
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems, extract_resource::ExtractResourcePlugin,
        render_resource::Extent3d,
    },
};

use wayland_client::{Connection, EventQueue, Proxy, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::{
    LiveWallpaperCamera, PointerButton, PointerSample, WallpaperPointerState, WallpaperSurfaceInfo,
    WallpaperTargetMonitor,
};
use std::collections::HashSet;

use super::{
    PendingPointerEvent, WaylandAppState,
    render::{
        WaylandGpuSurfaceState, WaylandRenderTarget, WaylandSurfaceDescriptor,
        create_wayland_image, prepare_wayland_surface, present_wayland_surface,
    },
};

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

        // At startup, create surfaces for the currently requested target monitor if available.
        let initial_target = app
            .world()
            .get_resource::<WallpaperTargetMonitor>()
            .copied()
            .unwrap_or_default();
        ensure_surfaces_for_outputs(&mut app_state, &qh, &initial_target);
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
    target_monitor: Res<WallpaperTargetMonitor>,
    mut pointer_state: ResMut<WallpaperPointerState>,
    mut surface_info: ResMut<WallpaperSurfaceInfo>,
) {
    if app_state.is_running() {
        if let Err(err) = event_queue.blocking_dispatch(&mut app_state) {
            warn!("Wayland event dispatch failed: {err:?}; closing background surface");
            app_state.closed = true;
            surface_descriptor.surfaces.clear();
            surface_descriptor.bump_generation();
            return;
        }

        let qh = event_queue.handle();
        let (mut touched, removed) =
            ensure_surfaces_for_outputs(&mut app_state, &qh, &target_monitor);

        if !removed.is_empty() {
            surface_descriptor
                .surfaces
                .retain(|s| !removed.contains(&s.output));
            touched = true;
        }

        for surface_config in app_state.take_surface_config() {
            info!(
                "Wayland surface configured (output {}): {}x{}",
                surface_config.output, surface_config.width, surface_config.height
            );
            surface_descriptor.upsert_surface(surface_config);
            touched = true;
        }

        // Integrate fresh logical positions/sizes from xdg-output / wl_output.
        if apply_output_info_updates(&mut surface_descriptor, &mut app_state) {
            touched = true;
        }

        if touched {
            surface_descriptor.bump_generation();
        }

        apply_pointer_events(
            &mut pointer_state,
            app_state.pending_pointer_events.drain(..),
        );

        if let Some((min_x, min_y, w, h)) =
            ready_bounds(&surface_descriptor, &app_state, &target_monitor)
        {
            surface_info.set(min_x, min_y, w, h);
        }
    }
}

fn ready_bounds(
    descriptor: &WaylandSurfaceDescriptor,
    app_state: &WaylandAppState,
    target: &WallpaperTargetMonitor,
) -> Option<(i32, i32, u32, u32)> {
    let selected = selected_outputs(app_state, target)?;

    let have_all_selected = selected.iter().all(|output| {
        descriptor
            .surfaces
            .iter()
            .find(|s| s.output == *output)
            .map(|entry| entry.handles.is_some() && entry.width > 0 && entry.height > 0)
            .unwrap_or(false)
    });
    if !have_all_selected {
        return None;
    }

    let missing_output_for_all = matches!(target, WallpaperTargetMonitor::All)
        && descriptor
            .surfaces
            .iter()
            .filter(|s| s.handles.is_some())
            .count()
            < app_state.outputs.len();
    if missing_output_for_all {
        return None;
    }

    descriptor.overall_bounds()
}

fn apply_pointer_events(
    state: &mut WallpaperPointerState,
    pending: impl IntoIterator<Item = PendingPointerEvent>,
) {
    for evt in pending {
        let prev_position = state
            .last
            .as_ref()
            .map(|s| s.position)
            .unwrap_or(evt.position + evt.offset);
        let new_position = evt.position + evt.offset;

        let mut sample = PointerSample {
            output: Some(evt.output),
            position: new_position,
            delta: new_position - prev_position,
            ..state.last.clone().unwrap_or_default()
        };

        sample.last_button = evt
            .kind
            .button_change()
            .map(|(button, pressed)| PointerButton { button, pressed });

        if let Some(btn) = sample.last_button
            && let Some(button) = btn.button
        {
            if btn.pressed {
                sample.pressed.insert(button);
            } else {
                sample.pressed.remove(&button);
            }
        }

        state.last = Some(sample);
    }
}

/// Apply the latest logical position/size info to existing surface descriptors.
/// Returns true if any descriptor changed.
fn apply_output_info_updates(
    descriptor: &mut WaylandSurfaceDescriptor,
    app_state: &mut WaylandAppState,
) -> bool {
    if app_state.dirty_outputs.is_empty() {
        return false;
    }

    let mut changed_any = false;

    #[inline]
    fn update_if<T: PartialEq + Copy>(dst: &mut T, src: T, changed: &mut bool) {
        if *dst != src {
            *dst = src;
            *changed = true;
        }
    }

    for surface in &mut descriptor.surfaces {
        if !app_state.dirty_outputs.contains(&surface.output) {
            continue;
        }

        if let Some(info) = app_state.output_info.get(&surface.output) {
            let mut changed = false;

            update_if(&mut surface.offset_x, info.x, &mut changed);
            update_if(&mut surface.offset_y, info.y, &mut changed);

            if info.width > 0 {
                update_if(&mut surface.width, info.width as u32, &mut changed);
            }
            if info.height > 0 {
                update_if(&mut surface.height, info.height as u32, &mut changed);
            }

            changed_any |= changed;
        }
    }

    app_state.dirty_outputs.clear();
    changed_any
}

fn sync_wayland_render_target_image(
    descriptor: Res<WaylandSurfaceDescriptor>,
    mut target: ResMut<WaylandRenderTarget>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some((_, _, width, height)) = descriptor.overall_bounds() else {
        return;
    };

    if target.last_applied_generation == descriptor.generation {
        return;
    }

    if let Some(image) = images.get_mut(&target.image) {
        let size = Extent3d {
            width,
            height,
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

/// Ensure we have a layer-surface for every known output.
/// Returns (touched, removed_outputs).
fn ensure_surfaces_for_outputs(
    app_state: &mut WaylandAppState,
    qh: &QueueHandle<WaylandAppState>,
    target: &WallpaperTargetMonitor,
) -> (bool, Vec<u32>) {
    let mut touched = false;
    let mut removed: Vec<u32> = Vec::new();

    let Some(compositor) = app_state.compositor.as_ref() else {
        return (touched, removed);
    };
    let Some(layer_shell) = app_state.layer_shell.as_ref() else {
        return (touched, removed);
    };

    let Some(selected) = selected_outputs(app_state, target) else {
        // Invalid selection (e.g., Index out of range); keep current surfaces as-is.
        return (touched, removed);
    };

    // create missing surfaces
    for output_name in &selected {
        let Some(output) = app_state.outputs.get(output_name) else {
            continue;
        };
        if app_state.surfaces.contains_key(output_name) {
            continue;
        }
        let surface = compositor.0.create_surface(qh, ());
        let surface_id = surface.id().protocol_id();
        let layer_surface = layer_shell.0.get_layer_surface(
            &surface,
            Some(output),
            zwlr_layer_shell_v1::Layer::Bottom,
            format!("egl_background_{output_name}"),
            qh,
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
        surface.commit();
        app_state.surfaces.insert(
            *output_name,
            super::OutputSurface {
                surface: surface.clone(),
                layer_surface,
            },
        );
        app_state.surface_to_output.insert(surface_id, *output_name);
        touched = true;
    }

    // remove surfaces whose outputs vanished
    let outputs: HashSet<u32> = selected.into_iter().collect();
    let to_remove: Vec<u32> = app_state
        .surfaces
        .keys()
        .filter(|k| !outputs.contains(k))
        .copied()
        .collect();
    for key in to_remove {
        if let Some(surface) = app_state.surfaces.remove(&key) {
            // Explicitly destroy to stop showing on that output.
            surface.layer_surface.destroy();
            surface.surface.destroy();
            app_state
                .surface_to_output
                .remove(&surface.surface.id().protocol_id());
        }
        touched = true;
        removed.push(key);
    }

    (touched, removed)
}

/// Choose outputs according to target monitor selection.
fn selected_outputs(
    app_state: &WaylandAppState,
    target: &WallpaperTargetMonitor,
) -> Option<Vec<u32>> {
    let mut outputs: Vec<u32> = app_state.output_order.clone();
    outputs.retain(|id| app_state.outputs.contains_key(id));

    match target {
        WallpaperTargetMonitor::All => Some(outputs),
        WallpaperTargetMonitor::Primary => {
            let v: Vec<u32> = outputs.into_iter().take(1).collect();
            if v.is_empty() { None } else { Some(v) }
        }
        WallpaperTargetMonitor::Index(n) => {
            let v: Vec<u32> = outputs.into_iter().skip(*n).take(1).collect();
            if v.is_empty() { None } else { Some(v) }
        }
    }
}
