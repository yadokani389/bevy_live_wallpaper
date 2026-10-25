use std::io::ErrorKind;

use bevy::{
    camera::CameraUpdateSystems,
    prelude::*,
    render::{Render, RenderApp, RenderSystems, extract_resource::ExtractResourcePlugin},
};
use wayland_client::{Connection, EventQueue, Proxy, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::{
    PointerButton, PointerSample, WallpaperPointerState, WallpaperSurfaceInfo,
    WallpaperTargetMonitor,
    render_target::{
        WallpaperRenderTarget, WallpaperRenderTargetPlugin, assign_wallpaper_camera_target,
    },
};

use super::{
    PendingPointerEvent, WaylandAppState,
    render::{
        WaylandGpuSurfaceState, WaylandSurfaceDescriptor, prepare_wayland_surface,
        present_wayland_surface,
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
                present_wayland_surface
                    .in_set(RenderSystems::Cleanup)
                    .after(RenderSystems::Render),
            );

        app.init_resource::<WaylandSurfaceDescriptor>()
            .add_plugins((
                ExtractResourcePlugin::<WaylandSurfaceDescriptor>::default(),
                WallpaperRenderTargetPlugin,
            ))
            .add_systems(PostUpdate, wayland_event_system)
            .add_systems(
                PostUpdate,
                (
                    sync_wayland_render_target_image.after(wayland_event_system),
                    assign_wallpaper_camera_target.after(sync_wayland_render_target_image),
                )
                    .before(CameraUpdateSystems),
            )
            .insert_non_send(WaylandEventQueue(event_queue))
            .insert_non_send(app_state);
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
        if let Err(err) = pump_wayland_events(&mut event_queue, &mut app_state) {
            warn!("Wayland event dispatch failed: {err:?}; closing background surface");
            app_state.closed = true;
            surface_descriptor.surfaces.clear();
            return;
        }

        let qh = event_queue.handle();
        ensure_surfaces_for_outputs(&mut app_state, &qh, &target_monitor);
        sync_surface_descriptor(&mut surface_descriptor, &mut app_state);

        if let Some(sample) = pointer_state.last.as_mut() {
            sample.delta = Vec2::ZERO;
            sample.last_button = None;
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

fn sync_surface_descriptor(
    descriptor: &mut WaylandSurfaceDescriptor,
    app_state: &mut WaylandAppState,
) {
    descriptor
        .surfaces
        .retain(|surface| app_state.surfaces.contains_key(&surface.output));

    for config in app_state.take_surface_config() {
        descriptor.upsert_surface(config);
    }
    apply_output_info_updates(descriptor, app_state);
}

fn pump_wayland_events(
    event_queue: &mut WaylandEventQueue,
    app_state: &mut WaylandAppState,
) -> Result<(), wayland_client::DispatchError> {
    while event_queue.dispatch_pending(app_state)? > 0 {}

    event_queue.flush()?;

    if let Some(read_guard) = event_queue.prepare_read() {
        match read_guard.read() {
            Ok(_) => while event_queue.dispatch_pending(app_state)? > 0 {},
            Err(wayland_client::backend::WaylandError::Io(err))
                if err.kind() == ErrorKind::WouldBlock => {}
            Err(err) => return Err(err.into()),
        }
    } else {
        while event_queue.dispatch_pending(app_state)? > 0 {}
    }

    Ok(())
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
        let new_position = evt.position + evt.offset;
        let sample = state.last.get_or_insert_with(|| PointerSample {
            position: new_position,
            ..default()
        });
        sample.output = Some(evt.output);
        sample.delta = new_position - sample.position;
        sample.position = new_position;

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
    }
}

fn apply_output_info_updates(
    descriptor: &mut WaylandSurfaceDescriptor,
    app_state: &mut WaylandAppState,
) {
    for surface in &mut descriptor.surfaces {
        if !app_state.dirty_outputs.contains(&surface.output) {
            continue;
        }
        if let Some(info) = app_state.output_info.get(&surface.output) {
            surface.offset_x = info.x;
            surface.offset_y = info.y;
            if info.width > 0 {
                surface.width = info.width as u32;
            }
            if info.height > 0 {
                surface.height = info.height as u32;
            }
        }
    }
    app_state.dirty_outputs.clear();
}

fn sync_wayland_render_target_image(
    descriptor: Res<WaylandSurfaceDescriptor>,
    target: Res<WallpaperRenderTarget>,
    mut images: ResMut<Assets<Image>>,
) {
    if let Some((_, _, width, height)) = descriptor.overall_bounds() {
        target.resize(&mut images, width, height);
    }
}

fn ensure_surfaces_for_outputs(
    app_state: &mut WaylandAppState,
    qh: &QueueHandle<WaylandAppState>,
    target: &WallpaperTargetMonitor,
) {
    let Some(compositor) = app_state.compositor.as_ref() else {
        return;
    };
    let Some(layer_shell) = app_state.layer_shell.as_ref() else {
        return;
    };

    let Some(selected) = selected_outputs(app_state, target) else {
        // Invalid selection (e.g., Index out of range); keep current surfaces as-is.
        return;
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
    }
    let to_remove: Vec<u32> = app_state
        .surfaces
        .keys()
        .filter(|key| !selected.contains(key))
        .copied()
        .collect();
    for output in to_remove {
        app_state.remove_surface(output);
    }
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

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixStream;

    use super::*;
    use crate::wayland::{OutputInfo, OutputSurface, PointerFocus, render::SurfaceDescriptorEntry};
    use wayland_client::protocol::{wl_output, wl_surface};

    #[test]
    fn pointer_motion_preserves_buttons_without_repeating_button_changes() {
        use crate::wayland::PendingPointerEventKind;

        let mut state = WallpaperPointerState::default();
        let event = |position, kind| PendingPointerEvent {
            output: 10,
            position,
            offset: Vec2::new(-1920.0, 0.0),
            kind,
        };
        apply_pointer_events(
            &mut state,
            [event(
                Vec2::ZERO,
                PendingPointerEventKind::Button {
                    button: Some(MouseButton::Left),
                    pressed: true,
                },
            )],
        );
        let sample = state.last.as_ref().unwrap();
        assert_eq!(sample.position, Vec2::new(-1920.0, 0.0));
        assert_eq!(sample.delta, Vec2::ZERO);
        assert!(sample.pressed.contains(&MouseButton::Left));
        apply_pointer_events(
            &mut state,
            [event(
                Vec2::new(20.0, 30.0),
                PendingPointerEventKind::Motion,
            )],
        );
        let sample = state.last.as_ref().unwrap();
        assert_eq!(sample.delta, Vec2::new(20.0, 30.0));
        assert!(sample.pressed.contains(&MouseButton::Left));
        assert!(sample.last_button.is_none());
        apply_pointer_events(
            &mut state,
            [event(
                Vec2::new(20.0, 30.0),
                PendingPointerEventKind::Button {
                    button: Some(MouseButton::Left),
                    pressed: false,
                },
            )],
        );
        assert!(state.last.as_ref().unwrap().pressed.is_empty());
    }

    #[test]
    fn removed_output_is_reconciled_after_registry_cleanup() {
        let (socket, _server) = UnixStream::pair().unwrap();
        let connection = Connection::from_socket(socket).unwrap();
        let backend = connection.backend().downgrade();
        let mut state = WaylandAppState::new(connection.display());
        let mut descriptor = WaylandSurfaceDescriptor::default();
        for output in [10, 20] {
            state
                .outputs
                .insert(output, wl_output::WlOutput::inert(backend.clone()));
            state.output_order.push(output);
            state.output_info.insert(output, OutputInfo::default());
            state.dirty_outputs.insert(output);
            state.surfaces.insert(
                output,
                OutputSurface {
                    surface: wl_surface::WlSurface::inert(backend.clone()),
                    layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1::inert(
                        backend.clone(),
                    ),
                },
            );
            state.surface_to_output.insert(output + 100, output);
            descriptor.surfaces.push(SurfaceDescriptorEntry {
                output,
                handles: None,
                width: 1920,
                height: 1080,
                offset_x: 0,
                offset_y: 0,
            });
        }
        state.pointer_focus = Some(PointerFocus {
            output: 10,
            position: Vec2::ZERO,
        });
        let queue = connection.new_event_queue::<WaylandAppState>();
        <WaylandAppState as wayland_client::Dispatch<wl_output::WlOutput, u32>>::event(
            &mut state,
            &wl_output::WlOutput::inert(backend),
            wl_output::Event::Scale { factor: 2 },
            &20,
            &connection,
            &queue.handle(),
        );
        assert_eq!(state.output_info[&20].scale, 2);
        assert_eq!(state.output_info.len(), 2);
        state.remove_output(10);
        sync_surface_descriptor(&mut descriptor, &mut state);

        assert_eq!(descriptor.surfaces.len(), 1);
        assert_eq!(descriptor.surfaces[0].output, 20);
        assert!(!state.outputs.contains_key(&10));
        assert!(!state.output_info.contains_key(&10));
        assert!(!state.surface_to_output.values().any(|output| *output == 10));
        assert_eq!(state.output_order, [20]);
        assert!(state.pointer_focus.is_none());
        sync_surface_descriptor(&mut descriptor, &mut state);
    }
}
