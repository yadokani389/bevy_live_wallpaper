use bevy::{
    asset::RenderAssetUsages,
    log::{debug, error, warn},
    prelude::{Assets, Handle, Image, Res, ResMut, Resource},
    render::{
        extract_resource::ExtractResource,
        render_asset::RenderAssets,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        renderer::{RenderAdapter, RenderDevice, RenderInstance, RenderQueue},
        texture::GpuImage,
    },
};
use wgpu::{
    CommandEncoderDescriptor, CompositeAlphaMode, Origin3d, PresentMode, SurfaceConfiguration,
    SurfaceError, SurfaceTargetUnsafe, TextureAspect,
};

use crate::x11::surface::X11SurfaceHandles;

pub const X11_SURFACE_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

pub(crate) fn create_x11_image(images: &mut Assets<Image>) -> Handle<Image> {
    let size = Extent3d {
        width: 1,
        height: 1,
        depth_or_array_layers: 1,
    };
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 255],
        X11_SURFACE_FORMAT,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;
    images.add(image)
}

#[derive(Resource, ExtractResource, Clone, Debug, Default)]
pub(crate) struct X11SurfaceDescriptor {
    pub handles: Option<X11SurfaceHandles>,
    pub width: u32,
    pub height: u32,
    pub generation: u64,
}

impl X11SurfaceDescriptor {
    pub(crate) fn new() -> Self {
        Self {
            handles: None,
            width: 0,
            height: 0,
            generation: 0,
        }
    }

    pub(crate) fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

#[derive(Resource, ExtractResource, Clone, Debug)]
pub(crate) struct X11RenderTarget {
    pub image: Handle<Image>,
    pub last_applied_generation: u64,
}

impl X11RenderTarget {
    pub(crate) fn new(image: Handle<Image>) -> Self {
        Self {
            image,
            last_applied_generation: 0,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct X11GpuSurfaceState {
    pub surface: Option<wgpu::Surface<'static>>,
    pub config: Option<SurfaceConfiguration>,
    pub last_applied_generation: u64,
}

impl X11GpuSurfaceState {
    pub(crate) fn mark_stale(&mut self) {
        self.surface = None;
        self.config = None;
        self.last_applied_generation = 0;
    }
}

pub(crate) fn prepare_x11_surface(
    descriptor: Res<X11SurfaceDescriptor>,
    mut state: ResMut<X11GpuSurfaceState>,
    render_instance: Res<RenderInstance>,
    render_adapter: Res<RenderAdapter>,
    render_device: Res<RenderDevice>,
) {
    if descriptor.handles.is_none() {
        if state.surface.is_some() {
            debug!("X11 surface handles dropped; tearing down wgpu surface");
        }
        state.mark_stale();
        return;
    }

    if descriptor.width == 0 || descriptor.height == 0 {
        return;
    }

    let needs_recreate =
        state.surface.is_none() || state.last_applied_generation != descriptor.generation;

    if needs_recreate {
        let handles = descriptor.handles.expect("handles exist");
        let raw_display_handle = handles.raw_display_handle();
        let raw_window_handle = handles.raw_window_handle();
        let instance = render_instance.0.as_ref();
        let surface = unsafe {
            instance
                .create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle,
                    raw_window_handle,
                })
                .expect("failed to create X11 wgpu surface")
        };
        state.surface = Some(surface);
    }

    let Some(surface) = state.surface.as_ref() else {
        return;
    };

    let width = descriptor.width.max(1);
    let height = descriptor.height.max(1);

    let needs_reconfigure = state
        .config
        .as_ref()
        .map(|config| config.width != width || config.height != height)
        .unwrap_or(true);

    if needs_reconfigure || needs_recreate {
        let capabilities = surface.get_capabilities(render_adapter.0.as_ref());
        if capabilities.formats.is_empty() {
            warn!("X11 surface reported no supported formats; retrying later");
            state.mark_stale();
            return;
        }

        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|fmt| *fmt == X11_SURFACE_FORMAT)
            .or_else(|| capabilities.formats.first().copied())
            .expect("X11 surface has no supported formats");

        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| matches!(mode, PresentMode::Mailbox | PresentMode::Immediate))
            .unwrap_or(PresentMode::Fifo);

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
        };

        render_device.configure_surface(surface, &config);

        state.config = Some(config);
    }

    state.last_applied_generation = descriptor.generation;
}

pub(crate) fn present_x11_surface(
    mut state: ResMut<X11GpuSurfaceState>,
    target: Option<Res<X11RenderTarget>>,
    images: Res<RenderAssets<GpuImage>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
) {
    let Some(target) = target else {
        return;
    };

    let Some(surface) = state.surface.as_ref() else {
        return;
    };

    let Some(config) = state.config.as_ref() else {
        return;
    };

    let Some(gpu_image) = images.get(&target.image) else {
        return;
    };

    let extent = Extent3d {
        width: config.width.min(gpu_image.size.width),
        height: config.height.min(gpu_image.size.height),
        depth_or_array_layers: 1,
    };

    let surface_texture = match surface.get_current_texture() {
        Ok(texture) => texture,
        Err(SurfaceError::Outdated | SurfaceError::Lost) => {
            warn!("X11 surface outdated/lost; scheduling recreate");
            state.mark_stale();
            return;
        }
        Err(SurfaceError::Timeout) => {
            debug!("X11 surface acquire timeout");
            return;
        }
        Err(SurfaceError::OutOfMemory) => {
            error!("X11 surface out of memory; disabling");
            state.mark_stale();
            return;
        }
        Err(other) => {
            warn!("Unexpected X11 surface error: {other:?}");
            return;
        }
    };

    let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("x11-surface-present"),
    });

    encoder.copy_texture_to_texture(
        gpu_image.texture.as_image_copy(),
        wgpu::TexelCopyTextureInfo {
            texture: &surface_texture.texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        extent,
    );

    render_queue.submit(Some(encoder.finish()));
    surface_texture.present();
}
