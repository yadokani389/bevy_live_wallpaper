use bevy::{
    asset::RenderAssetUsages,
    camera::RenderTarget,
    prelude::*,
    render::{
        RenderApp,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    },
};

use crate::LiveWallpaperCamera;

pub(crate) const WALLPAPER_SURFACE_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

pub(crate) struct WallpaperRenderTargetPlugin;

impl Plugin for WallpaperRenderTargetPlugin {
    fn build(&self, app: &mut App) {
        let target =
            WallpaperRenderTarget::new(&mut app.world_mut().resource_mut::<Assets<Image>>());
        app.insert_resource(target)
            .add_plugins(ExtractResourcePlugin::<WallpaperRenderTarget>::default());
    }
}

#[derive(Resource, ExtractResource, Clone, Debug)]
#[extract_app(RenderApp)]
pub(crate) struct WallpaperRenderTarget {
    pub image: Handle<Image>,
}

impl WallpaperRenderTarget {
    fn new(images: &mut Assets<Image>) -> Self {
        let mut image = Image::new_fill(
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 255],
            WALLPAPER_SURFACE_FORMAT,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC;
        Self {
            image: images.add(image),
        }
    }

    pub(crate) fn resize(&self, images: &mut Assets<Image>, width: u32, height: u32) {
        let size = Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        if width == 0
            || height == 0
            || images
                .get(&self.image)
                .is_none_or(|image| image.texture_descriptor.size == size)
        {
            return;
        }
        if let Some(mut image) = images.get_mut(&self.image) {
            image.resize(size);
        }
    }
}

pub(crate) fn assign_wallpaper_camera_target(
    target: Res<WallpaperRenderTarget>,
    mut commands: Commands,
    cameras: Query<(Entity, Option<&RenderTarget>), With<LiveWallpaperCamera>>,
) {
    for (entity, current) in &cameras {
        if current.and_then(RenderTarget::as_image) != Some(&target.image) {
            commands
                .entity(entity)
                .insert(RenderTarget::Image(target.image.clone().into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct TargetChanges(usize);

    #[test]
    fn image_resize_and_camera_assignment_preserve_unchanged_targets() {
        let mut images = Assets::<Image>::default();
        let target = WallpaperRenderTarget::new(&mut images);
        target.resize(&mut images, 320, 180);
        let image = images.get(&target.image).unwrap();
        assert_eq!(image.size(), UVec2::new(320, 180));
        assert_eq!(image.data.as_ref().unwrap().len(), 320 * 180 * 4);
        target.resize(&mut images, 0, 0);
        assert_eq!(
            images.get(&target.image).unwrap().size(),
            UVec2::new(320, 180)
        );

        let mut app = App::new();
        app.insert_resource(target)
            .init_resource::<TargetChanges>()
            .add_systems(
                Update,
                (
                    assign_wallpaper_camera_target,
                    |changed: Query<Entity, Changed<RenderTarget>>,
                     mut count: ResMut<TargetChanges>| {
                        count.0 = changed.iter().count();
                    },
                )
                    .chain(),
            );
        let first = app.world_mut().spawn(LiveWallpaperCamera).id();
        app.update();
        assert_eq!(app.world().resource::<TargetChanges>().0, 1);
        app.update();
        assert_eq!(app.world().resource::<TargetChanges>().0, 0);
        app.world_mut().spawn(LiveWallpaperCamera);
        app.update();
        assert_eq!(app.world().resource::<TargetChanges>().0, 1);
        app.world_mut()
            .entity_mut(first)
            .insert(RenderTarget::default());
        app.update();
        let target = app.world().resource::<WallpaperRenderTarget>();
        assert_eq!(
            app.world().get::<RenderTarget>(first).unwrap().as_image(),
            Some(&target.image)
        );
    }
}
