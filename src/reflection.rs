//! Capture the static forest once, then release all six capture cameras.
use crate::{effects, scene};
use bevy::{
    asset::RenderAssetUsages,
    camera::{Exposure, Hdr, RenderTarget, visibility::RenderLayers},
    core_pipeline::tonemapping::Tonemapping,
    light::{EnvironmentMapLight, ShadowFilteringMethod, Skybox},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Resource, Clone, ExtractResource)]
#[extract_app(RenderApp)]
pub struct Reflection {
    pub cube: Handle<Image>,
    faces: Vec<Handle<Image>>,
    ready: Arc<AtomicBool>,
    face: Arc<AtomicUsize>,
    active: usize,
    frames: u32,
}
impl Reflection {
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}
#[derive(Component)]
struct ProbeCamera(usize);

pub struct ReflectionPlugin;
impl Plugin for ReflectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<Reflection>::default())
            .add_systems(Startup, setup.before(effects::setup))
            .add_systems(Update, update);
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(Render, copy.after(RenderSystems::Render));
        }
    }
}
fn setup(mut commands: Commands, server: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    let size = Extent3d {
        width: 256,
        height: 256,
        depth_or_array_layers: 6,
    };
    let mut cube = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0; 8],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::RENDER_WORLD,
    );
    cube.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    cube.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    let cube = images.add(cube);
    let mut faces = Vec::new();
    let center = effects::origin(5) + Vec3::new(0.75, 0.8, 1.9);
    for (index, (direction, up)) in [
        (Vec3::X, Vec3::Y),
        (-Vec3::X, Vec3::Y),
        (Vec3::Y, -Vec3::Z),
        (-Vec3::Y, Vec3::Z),
        (Vec3::Z, Vec3::Y),
        (-Vec3::Z, Vec3::Y),
    ]
    .into_iter()
    .enumerate()
    {
        let mut face = Image::new_target_texture(256, 256, TextureFormat::Rgba16Float, None);
        face.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let face = images.add(face);
        faces.push(face.clone());
        commands.spawn((
            Camera3d::default(),
            Camera {
                is_active: false,
                order: -1,
                ..default()
            },
            Hdr,
            RenderTarget::Image(face.into()),
            Msaa::Off,
            ShadowFilteringMethod::Gaussian,
            ProbeCamera(index),
            Projection::Perspective(PerspectiveProjection {
                fov: std::f32::consts::FRAC_PI_2,
                near: 0.05,
                far: 40.,
                aspect_ratio: 1.,
                ..default()
            }),
            Transform::from_translation(center).looking_to(direction, up),
            Exposure {
                ev100: (1.0_f32 / (1.2 * 0.65)).log2(),
            },
            Tonemapping::None,
            bevy::core_pipeline::tonemapping::DebandDither::Disabled,
            RenderLayers::layer(0),
            Skybox {
                image: Some(scene::image(&server, "sky", false)),
                brightness: 0.5,
                ..default()
            },
            EnvironmentMapLight {
                diffuse_map: scene::image(&server, "diffuse", false),
                specular_map: scene::image(&server, "specular", false),
                intensity: 0.18 * crate::LIGHT_SCALE,
                ..default()
            },
        ));
    }
    commands.insert_resource(Reflection {
        cube,
        faces,
        ready: Arc::new(AtomicBool::new(false)),
        face: Arc::new(AtomicUsize::new(0)),
        active: 0,
        frames: 0,
    });
}
fn update(
    mut commands: Commands,
    forest: Res<scene::Forest>,
    mut reflection: ResMut<Reflection>,
    mut cameras: Query<(Entity, &mut Camera, &ProbeCamera)>,
) {
    if reflection.ready.load(Ordering::Acquire) {
        for (entity, _, _) in &cameras {
            commands.entity(entity).despawn();
        }
        if !reflection.faces.is_empty() {
            reflection.faces.clear();
        }
        return;
    }
    if !forest.ready {
        return;
    }
    if reflection.frames == 0 {
        info!("Starting one-time forest reflection");
    }
    reflection.frames += 1;
    let current = reflection.face.load(Ordering::Acquire);
    reflection.active = current;
    for (_, mut camera, probe) in &mut cameras {
        camera.is_active = probe.0 == current;
    }
}
fn copy(
    reflection: Option<Res<Reflection>>,
    images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
    pipelines: Res<PipelineCache>,
    mut stable: Local<u32>,
) {
    let Some(reflection) = reflection else {
        return;
    };
    if reflection.frames < 2 || reflection.ready.load(Ordering::Acquire) {
        return;
    }
    let waiting = pipelines.waiting_pipelines().count();
    if waiting > 0 {
        *stable = 0;
        if reflection.frames == 2 || reflection.frames.is_multiple_of(120) {
            info!("Reflection waiting for {waiting} shader pipelines");
        }
        return;
    }
    // One ready frame can still have skipped newly compiled draw items.
    *stable += 1;
    if *stable < 2 {
        return;
    }
    let Some(cube) = images.get(&reflection.cube) else {
        return;
    };
    let layer = reflection.active;
    if layer != reflection.face.load(Ordering::Acquire) {
        return;
    }
    let Some(face) = images.get(&reflection.faces[layer]) else {
        return;
    };
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("One-time forest reflection"),
    });
    encoder.copy_texture_to_texture(
        face.texture.as_image_copy(),
        TexelCopyTextureInfo {
            texture: &cube.texture,
            mip_level: 0,
            origin: Origin3d {
                x: 0,
                y: 0,
                z: layer as u32,
            },
            aspect: TextureAspect::All,
        },
        Extent3d {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    info!("Captured forest reflection face {layer}");
    reflection.face.store(layer + 1, Ordering::Release);
    if layer == 5 {
        reflection.ready.store(true, Ordering::Release);
    }
}
