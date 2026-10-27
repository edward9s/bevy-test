use bevy::{
    mesh::MeshVertexBufferLayoutRef,
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::{ShaderDefVal, ShaderRef},
};

pub type Foliage = ExtendedMaterial<StandardMaterial, Wind>;
pub type Water = ExtendedMaterial<StandardMaterial, Ripples>;

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Wind {
    #[uniform(100)]
    pub canopy: u32,
}
impl MaterialExtension for Wind {
    fn vertex_shader() -> ShaderRef {
        "shaders/foliage.wesl".into()
    }
    fn prepass_vertex_shader() -> ShaderRef {
        "shaders/foliage.wesl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/foliage_fragment.wesl".into()
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct Ripples {
    #[texture(100, dimension = "cube")]
    #[sampler(101)]
    pub reflection: Handle<Image>,
}
impl MaterialExtension for Ripples {
    fn vertex_shader() -> ShaderRef {
        "shaders/water_vertex.wesl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/water_fragment.wesl".into()
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
#[bind_group_data(ParticleKey)]
pub struct Particles {
    #[uniform(0)]
    pub kind: u32,
    #[uniform(0)]
    pub order: [UVec4; 27],
}
impl Material for Particles {
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let define = ShaderDefVal::UInt("EFFECT_KIND".into(), key.bind_group_data.0);
        descriptor.vertex.shader_defs.push(define.clone());
        descriptor
            .fragment
            .as_mut()
            .expect("Particle fragment missing")
            .shader_defs
            .push(define);
        Ok(())
    }

    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn vertex_shader() -> ShaderRef {
        "shaders/particles.wesl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/particles.wesl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct ParticleKey(u32);
impl From<&Particles> for ParticleKey {
    fn from(material: &Particles) -> Self {
        Self(material.kind)
    }
}
