use std::{collections::HashMap, sync::Arc};

use bevy::{
    log::{debug, error, warn},
    prelude::{Res, ResMut, Resource},
    render::{
        RenderApp,
        extract_resource::ExtractResource,
        render_asset::RenderAssets,
        render_resource::{Extent3d, TextureUsages},
        renderer::{RenderAdapter, RenderDevice, RenderInstance, RenderQueue},
        texture::GpuImage,
    },
};
use wgpu::{
    CommandEncoderDescriptor, CompositeAlphaMode, CurrentSurfaceTexture, Origin3d, PresentMode,
    SurfaceColorSpace, SurfaceConfiguration, SurfaceTargetUnsafe, TextureAspect,
};

use crate::{
    render_target::{WALLPAPER_SURFACE_FORMAT, WallpaperRenderTarget},
    wayland::{OutputSurface, surface::WaylandSurfaceHandles},
};

#[derive(Resource, ExtractResource, Clone, Debug, Default)]
#[extract_app(RenderApp)]
pub(crate) struct WaylandSurfaceDescriptor {
    pub surfaces: Vec<SurfaceDescriptorEntry>,
}

impl WaylandSurfaceDescriptor {
    pub(crate) fn upsert_surface(&mut self, config: crate::wayland::WaylandSurfaceConfig) {
        if let Some(entry) = self
            .surfaces
            .iter_mut()
            .find(|entry| entry.output == config.output)
        {
            entry.handles = Some(config.handles);
            entry.owner = config.owner;
            entry.width = config.width;
            entry.height = config.height;
            entry.offset_x = config.offset_x;
            entry.offset_y = config.offset_y;
        } else {
            self.surfaces.push(SurfaceDescriptorEntry {
                output: config.output,
                handles: Some(config.handles),
                owner: config.owner,
                width: config.width,
                height: config.height,
                offset_x: config.offset_x,
                offset_y: config.offset_y,
            });
        }
    }

    pub(crate) fn overall_bounds(&self) -> Option<(i32, i32, u32, u32)> {
        let mut iter_all = self.surfaces.iter().filter(|s| s.handles.is_some());
        let first = iter_all.next()?;

        let mut min_x = first.offset_x;
        let mut min_y = first.offset_y;
        let mut max_x = first.offset_x + first.width as i32;
        let mut max_y = first.offset_y + first.height as i32;

        for s in iter_all {
            min_x = min_x.min(s.offset_x);
            min_y = min_y.min(s.offset_y);
            max_x = max_x.max(s.offset_x + s.width as i32);
            max_y = max_y.max(s.offset_y + s.height as i32);
        }

        let width = (max_x - min_x).max(1) as u32;
        let height = (max_y - min_y).max(1) as u32;

        Some((min_x, min_y, width, height))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceDescriptorEntry {
    pub output: u32,
    pub handles: Option<WaylandSurfaceHandles>,
    pub owner: Arc<OutputSurface>,
    pub width: u32,
    pub height: u32,
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Resource, Default)]
pub(crate) struct WaylandGpuSurfaceState {
    pub surfaces: HashMap<u32, WaylandGpuPerSurface>,
}

#[derive(Default)]
pub(crate) struct WaylandGpuPerSurface {
    pub surface: Option<wgpu::Surface<'static>>,
    pub config: Option<SurfaceConfiguration>,
    // Field order keeps native objects alive until the GPU surface has been dropped.
    pub owner: Option<Arc<OutputSurface>>,
}

pub(crate) fn prepare_wayland_surface(
    descriptor: Res<WaylandSurfaceDescriptor>,
    mut state: ResMut<WaylandGpuSurfaceState>,
    render_instance: Res<RenderInstance>,
    render_adapter: Res<RenderAdapter>,
    render_device: Res<RenderDevice>,
) {
    state.surfaces.retain(|output, _| {
        descriptor
            .surfaces
            .iter()
            .any(|surface| surface.output == *output && surface.handles.is_some())
    });

    for surf_desc in descriptor.surfaces.iter().filter(|s| s.handles.is_some()) {
        let entry = state.surfaces.entry(surf_desc.output).or_default();

        let needs_recreate = entry.surface.is_none()
            || entry
                .owner
                .as_ref()
                .is_none_or(|owner| !Arc::ptr_eq(owner, &surf_desc.owner));

        if needs_recreate {
            let handles = surf_desc.handles.expect("handles exist");
            let raw_display_handle = handles.raw_display_handle();
            let raw_window_handle = handles.raw_window_handle();
            let surface = unsafe {
                render_instance
                    .create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
                        raw_display_handle: Some(raw_display_handle),
                        raw_window_handle,
                    })
                    .expect("failed to create Wayland wgpu surface")
            };
            entry.surface = Some(surface);
            entry.owner = Some(surf_desc.owner.clone());
            entry.config = None;
        }

        let Some(surface) = entry.surface.as_ref() else {
            continue;
        };

        let width = surf_desc.width.max(1);
        let height = surf_desc.height.max(1);

        let needs_reconfigure = entry
            .config
            .as_ref()
            .map(|config| config.width != width || config.height != height)
            .unwrap_or(true);

        if needs_reconfigure || needs_recreate {
            let capabilities = surface.get_capabilities(&render_adapter);
            if capabilities.formats.is_empty() {
                warn!("Wayland surface reported no supported formats; retrying later");
                entry.surface = None;
                entry.config = None;
                entry.owner = None;
                continue;
            }

            let format = capabilities
                .formats
                .iter()
                .copied()
                .find(|fmt| *fmt == WALLPAPER_SURFACE_FORMAT)
                .or_else(|| capabilities.formats.first().copied())
                .expect("Wayland surface has no supported formats");

            let present_mode = capabilities
                .present_modes
                .iter()
                .copied()
                .find(|mode| matches!(mode, PresentMode::Fifo))
                .or_else(|| capabilities.present_modes.first().copied())
                .expect("Wayland surface has no supported present mode");

            let alpha_mode = capabilities
                .alpha_modes
                .iter()
                .copied()
                .find(|mode| matches!(mode, CompositeAlphaMode::Opaque))
                .unwrap_or(capabilities.alpha_modes[0]);

            let mut usage = TextureUsages::RENDER_ATTACHMENT;
            if capabilities.usages.contains(TextureUsages::COPY_DST) {
                usage |= TextureUsages::COPY_DST;
            }

            let config = SurfaceConfiguration {
                usage,
                format,
                width,
                height,
                present_mode,
                alpha_mode,
                view_formats: vec![],
                desired_maximum_frame_latency: 1,
                color_space: SurfaceColorSpace::Auto,
            };

            render_device.configure_surface(surface, &config);

            entry.config = Some(config);
        }
    }
}

pub(crate) fn present_wayland_surface(
    mut state: ResMut<WaylandGpuSurfaceState>,
    target: Option<Res<WallpaperRenderTarget>>,
    images: Res<RenderAssets<GpuImage>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    descriptor: Res<WaylandSurfaceDescriptor>,
) {
    let Some(target) = target else { return };

    let Some(gpu_image) = images.get(&target.image) else {
        return;
    };

    let Some((min_x, min_y, _, _)) = descriptor.overall_bounds() else {
        return;
    };

    for (output, entry) in state.surfaces.iter_mut() {
        let Some(surface) = entry.surface.as_ref() else {
            continue;
        };
        let Some(config) = entry.config.as_ref() else {
            continue;
        };

        let Some(desc_entry) = descriptor
            .surfaces
            .iter()
            .find(|s| s.output == *output && s.handles.is_some())
        else {
            continue;
        };

        let extent = Extent3d {
            width: config.width.min(gpu_image.texture_descriptor.size.width),
            height: config.height.min(gpu_image.texture_descriptor.size.height),
            depth_or_array_layers: 1,
        };

        let surface_texture = match surface.get_current_texture() {
            CurrentSurfaceTexture::Success(texture)
            | CurrentSurfaceTexture::Suboptimal(texture) => texture,
            CurrentSurfaceTexture::Outdated => {
                debug!(
                    "Wayland surface for output {} outdated; scheduling reconfigure",
                    output
                );
                entry.config = None;
                continue;
            }
            CurrentSurfaceTexture::Lost => {
                warn!(
                    "Wayland surface for output {} lost; scheduling recreate",
                    output
                );
                entry.surface = None;
                entry.config = None;
                entry.owner = None;
                continue;
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => {
                debug!("Wayland surface acquire timeout (output {})", output);
                continue;
            }
            CurrentSurfaceTexture::Validation => {
                error!("Wayland surface validation failed (output {})", output);
                continue;
            }
        };

        let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("wayland-surface-present"),
        });

        let src_origin = Origin3d {
            x: (desc_entry.offset_x - min_x).max(0) as u32,
            y: (desc_entry.offset_y - min_y).max(0) as u32,
            z: 0,
        };

        let mut src = gpu_image.texture.as_image_copy();
        src.origin = src_origin;

        let dst = wgpu::TexelCopyTextureInfo {
            texture: &surface_texture.texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        };

        encoder.copy_texture_to_texture(src, dst, extent);

        render_queue.submit(Some(encoder.finish()));
        render_queue.present(surface_texture);
    }
}
