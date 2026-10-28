use crate::materials::{Foliage, Wind};
use bevy::{
    asset::{LoadState, RenderAssetUsages},
    camera::primitives::MeshAabb,
    gltf::{Gltf, GltfMesh},
    image::{
        ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler,
        ImageSamplerDescriptor,
    },
    light::NotShadowCaster,
    prelude::*,
};

#[derive(Resource)]
pub struct Forest {
    pub gltf: Handle<Gltf>,
    pub ready: bool,
    pub texture_format: &'static str,
    surfaces: Vec<Handle<StandardMaterial>>,
    leaves: Handle<Foliage>,
    canopy: Handle<Foliage>,
    textures: Vec<Handle<Image>>,
}

pub fn image(server: &AssetServer, name: &str, srgb: bool) -> Handle<Image> {
    texture(server, name, srgb, false)
}
fn texture(server: &AssetServer, name: &str, srgb: bool, astc: bool) -> Handle<Image> {
    server
        .load_builder()
        .with_settings(move |settings: &mut ImageLoaderSettings| {
            settings.is_srgb = srgb;
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                anisotropy_clamp: 4,
                ..default()
            });
        })
        .load(format!(
            "generated/{name}.{}",
            if astc { "ktx2" } else { "dds" }
        ))
}

pub fn load(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut foliage: ResMut<Assets<Foliage>>,
    device: Res<bevy::render::renderer::RenderDevice>,
) {
    let astc = device
        .features()
        .contains(bevy::render::render_resource::WgpuFeatures::TEXTURE_COMPRESSION_ASTC);
    info!(
        "Surface textures: {}",
        if astc { "ASTC 4x4" } else { "RGBA8" }
    );
    let mut textures = Vec::new();
    let surfaces = ["ground", "stone", "bark"]
        .iter()
        .map(|name| {
            let orm = texture(&server, &format!("{name}_arm"), false, astc);
            let albedo = texture(&server, &format!("{name}_albedo"), true, astc);
            let normal = texture(&server, &format!("{name}_normal"), false, astc);
            textures.extend([orm.clone(), albedo.clone(), normal.clone()]);
            materials.add(StandardMaterial {
                base_color_texture: Some(albedo),
                normal_map_texture: Some(normal),
                metallic_roughness_texture: Some(orm.clone()),
                occlusion_texture: Some(orm),
                metallic: 0.,
                perceptual_roughness: 1.,
                ..default()
            })
        })
        .collect();
    let leaves = foliage.add(Foliage {
        base: StandardMaterial {
            perceptual_roughness: 0.68,
            cull_mode: None,
            double_sided: true,
            diffuse_transmission: 0.65 * 0.45,
            ..default()
        },
        extension: Wind { canopy: 0 },
    });
    let canopy_image = texture(&server, "canopy", true, astc);
    textures.push(canopy_image.clone());
    textures.extend(["sky", "diffuse", "specular"].map(|name| image(&server, name, false)));
    let canopy = foliage.add(Foliage {
        base: StandardMaterial {
            base_color_texture: Some(canopy_image),
            perceptual_roughness: 0.8,
            cull_mode: None,
            double_sided: true,
            alpha_mode: AlphaMode::Mask(0.4),
            diffuse_transmission: 0.55 * 0.3,
            ..default()
        },
        extension: Wind { canopy: 1 },
    });
    commands.insert_resource(Forest {
        gltf: server.load("generated/forest.glb"),
        ready: false,
        texture_format: if astc { "ASTC 1024" } else { "RGBA8 1024" },
        surfaces,
        leaves,
        canopy,
        textures,
    });
}

pub fn spawn(
    mut commands: Commands,
    mut forest: ResMut<Forest>,
    server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    meshes: Res<Assets<GltfMesh>>,
    mut geometry: ResMut<Assets<Mesh>>,
) {
    if forest.ready {
        return;
    }
    if let Some(LoadState::Failed(error)) = server.get_load_state(forest.gltf.id()) {
        panic!("Forest asset failed to load: {error}");
    }
    for image in &forest.textures {
        if let Some(LoadState::Failed(error)) = server.get_load_state(image.id()) {
            panic!("Forest texture failed to load: {error}");
        }
    }
    if !server.is_loaded_with_dependencies(forest.gltf.id())
        || forest
            .textures
            .iter()
            .any(|image| !server.is_loaded_with_dependencies(image.id()))
    {
        return;
    }
    let gltf = gltfs.get(&forest.gltf).expect("Loaded forest missing glTF");
    assert!(
        (100..=500).contains(&gltf.meshes.len()),
        "Unexpected forest geometry"
    );
    for handle in &gltf.meshes {
        let mesh = meshes.get(handle).expect("Missing forest mesh");
        assert_eq!(mesh.primitives.len(), 1);
        let mut entity = commands.spawn((
            Name::new(mesh.name.clone()),
            Mesh3d(mesh.primitives[0].mesh.clone()),
            Transform::IDENTITY,
        ));
        let surface = mesh.name.split('_').next().unwrap();
        match surface {
            "Ground" => {
                entity.insert((MeshMaterial3d(forest.surfaces[0].clone()), NotShadowCaster));
            }
            "Stone" => {
                entity.insert(MeshMaterial3d(forest.surfaces[1].clone()));
            }
            "Bark" => {
                entity.insert(MeshMaterial3d(forest.surfaces[2].clone()));
            }
            "Leaves" => {
                entity.insert(MeshMaterial3d(forest.leaves.clone()));
            }
            "Canopy" => {
                entity.insert(MeshMaterial3d(forest.canopy.clone()));
            }
            _ => panic!("Unknown forest surface: {surface}"),
        }
        // The mesh bounds are already spatially batched in 10 m cells.
        // Wind moves by at most 0.082 m; keep the conservative culling bounds.
        let mut data = geometry
            .get_mut(&mesh.primitives[0].mesh)
            .expect("Missing geometry");
        let mut bounds = data.get_aabb().expect("Missing geometry bounds");
        if surface == "Leaves" || surface == "Canopy" {
            bounds.half_extents += bevy::math::Vec3A::splat(0.09);
        }
        entity.insert(bounds);
        // Static forest geometry is never edited by the CPU after upload.
        data.asset_usage = RenderAssetUsages::RENDER_WORLD;
    }
    forest.ready = true;
    info!("Forest ready: {} exact source batches", gltf.meshes.len());
}
