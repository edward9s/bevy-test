use crate::materials::{Particles, Ripples, Water};
use bevy::{
    asset::RenderAssetUsages,
    camera::primitives::Aabb,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

pub const SITES: [Vec2; 6] = [
    Vec2::new(-3.2, 3.1),
    Vec2::new(-6.3, -1.6),
    Vec2::new(3.5, 3.5),
    Vec2::new(6.3, -3.2),
    Vec2::new(-7., -6.2),
    Vec2::new(3.5, 3.5),
];
pub fn origin(kind: usize) -> Vec3 {
    let p = SITES[kind];
    let hills =
        0.36 * (p.x * 0.24).sin() * (p.y * 0.18).cos() + 0.12 * (p.x * 0.7 + p.y * 0.35).sin();
    let blend = ((p.x.abs() - 6.).max((p.y + 3.).abs() - 7.) / 5.).clamp(0., 1.);
    Vec3::new(p.x, hills * blend, p.y)
}
pub fn blast_age(time: f32) -> f32 {
    time % 5.4
}
pub fn pulse(time: f32) -> f32 {
    let phase = time % 4.8;
    (-((phase - 0.12) / 0.055).powi(2)).exp() + 0.75 * (-((phase - 0.3) / 0.08).powi(2)).exp()
}
#[derive(Component)]
pub struct EffectLight(pub usize);
#[derive(Component)]
pub struct EffectBatch(pub usize);

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut particles: ResMut<Assets<Particles>>,
    mut water: ResMut<Assets<Water>>,
    reflection: Res<crate::reflection::Reflection>,
) {
    for (kind, count) in [78, 108, 45, 36, 52].into_iter().enumerate() {
        let mut positions = Vec::with_capacity(count * 4);
        let mut uv = Vec::with_capacity(count * 4);
        let mut indices = Vec::with_capacity(count * 6);
        for i in 0..count {
            for corner in [[0., 0.], [1., 0.], [1., 1.], [0., 1.]] {
                positions.push([i as f32, 0., 0.]);
                uv.push(corner);
            }
            let n = (i * 4) as u32;
            indices.extend([n, n + 1, n + 2, n, n + 2, n + 3]);
        }
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0., 0., 1.]; count * 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_indices(Indices::U32(indices));
        commands.spawn((
            Name::new(["Fire", "Blast", "Spray", "Lightning", "Smoke"][kind]),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(particles.add(Particles {
                kind: kind as u32,
                order: std::array::from_fn(|i| {
                    UVec4::new(
                        (i * 4) as u32,
                        (i * 4 + 1) as u32,
                        (i * 4 + 2) as u32,
                        (i * 4 + 3) as u32,
                    )
                }),
            })),
            Transform::from_translation(origin(kind)),
            NotShadowCaster,
            EffectBatch(kind),
            bevy::camera::visibility::RenderLayers::layer(2),
            Aabb {
                center: Vec3::new(0., 10., 0.).into(),
                half_extents: Vec3::new(8., 12., 8.).into(),
            },
        ));
    }
    // Fixed topology. Only the shader moves the water vertices and ripple normals.
    let mut positions = Vec::<[f32; 3]>::new();
    let mut uv = Vec::<[f32; 2]>::new();
    let mut colors = Vec::<[f32; 4]>::new();
    let mut normals = Vec::<[f32; 3]>::new();
    let mut tangents = Vec::<[f32; 4]>::new();
    let mut indices = Vec::<u32>::new();
    let mut quad = |points: [Vec3; 4], coords: [[f32; 2]; 4], color: [f32; 4]| {
        let n = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize();
        let tangent = (points[1] - points[0]).normalize();
        let start = positions.len() as u32;
        for i in 0..4 {
            positions.push(points[i].to_array());
            uv.push(coords[i]);
            colors.push(color);
            normals.push(n.to_array());
            tangents.push([tangent.x, tangent.y, tangent.z, 1.]);
        }
        indices.extend([start, start + 1, start + 2, start, start + 2, start + 3]);
    };
    let water_point = |x: usize, z: usize| {
        Vec3::new(
            (x as f32 / 6. - 0.5) * 1.45 + 0.75,
            -0.085,
            z as f32 / 12. * 3.3 + 0.25,
        )
    };
    for z in 0..12 {
        for x in 0..6 {
            quad(
                [
                    water_point(x, z),
                    water_point(x + 1, z),
                    water_point(x + 1, z + 1),
                    water_point(x, z + 1),
                ],
                [
                    [x as f32 / 6., z as f32 / 12.],
                    [(x + 1) as f32 / 6., z as f32 / 12.],
                    [(x + 1) as f32 / 6., (z + 1) as f32 / 12.],
                    [x as f32 / 6., (z + 1) as f32 / 12.],
                ],
                [0.065, 0.17, 0.16, 0.62],
            );
        }
    }
    let jet = |i: usize| {
        let t = i as f32 / 24.;
        Vec3::new(t * 0.75, 1.15 + t * 0.55 - 1.8 * t * t, t * 1.45)
    };
    for i in 0..24 {
        let a = jet(i);
        let b = jet(i + 1);
        let side = (b - a).normalize().cross(Vec3::X).normalize();
        let up = (b - a).normalize().cross(side);
        for j in 0..8 {
            let ring = |n: usize| {
                let angle = std::f32::consts::TAU * n as f32 / 8.;
                (side * angle.cos() + up * angle.sin()) * 0.048
            };
            quad(
                [a + ring(j), a + ring(j + 1), b + ring(j + 1), b + ring(j)],
                [
                    [j as f32 / 8., i as f32 / 24.],
                    [(j + 1) as f32 / 8., i as f32 / 24.],
                    [(j + 1) as f32 / 8., (i + 1) as f32 / 24.],
                    [j as f32 / 8., (i + 1) as f32 / 24.],
                ],
                [0.16, 0.35, 0.38, 0.56],
            );
        }
    }
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tangents)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
    commands.spawn((
        Name::new("Water"),
        bevy::camera::visibility::RenderLayers::layer(1),
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(water.add(Water {
            base: StandardMaterial {
                alpha_mode: AlphaMode::Premultiplied,
                perceptual_roughness: 0.18,
                cull_mode: None,
                double_sided: true,
                ..default()
            },
            extension: Ripples {
                reflection: reflection.cube.clone(),
            },
        })),
        Transform::from_translation(origin(5)),
        NotShadowCaster,
        Aabb {
            center: Vec3::new(0.75, 0.5, 1.8).into(),
            half_extents: Vec3::new(2., 2., 3.).into(),
        },
    ));
    for (kind, color, intensity) in [
        (0, Color::linear_rgb(1., 0.32, 0.06), 15.),
        (1, Color::linear_rgb(1., 0.5, 0.1), 100.),
        (3, Color::linear_rgb(0.45, 0.65, 1.), 50.),
    ] {
        commands.spawn((
            PointLight {
                color,
                intensity: intensity * crate::LIGHT_SCALE,
                range: 6.,
                radius: 0.1,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(origin(kind) + Vec3::Y),
            EffectLight(kind),
        ));
    }
}

pub fn animate(
    time: Res<Time>,
    mut lights: Query<(&EffectLight, &mut PointLight)>,
    mut batches: Query<(&EffectBatch, &mut Visibility)>,
) {
    let t = time.elapsed_secs_wrapped();
    for (kind, mut light) in &mut lights {
        light.intensity = (match kind.0 {
            0 => 15. * (1. + 0.13 * (t * 13.).sin() + 0.09 * (t * 23.).sin()),
            1 => 100. * (-blast_age(t) * 7.).exp(),
            3 => 50. * pulse(t),
            _ => unreachable!(),
        }) * crate::LIGHT_SCALE;
    }
    for (kind, mut visible) in &mut batches {
        let shown = match kind.0 {
            1 => (1. - blast_age(t) / 2.5).max(0.) * 0.65 >= 0.003,
            3 => pulse(t) >= 0.003,
            _ => true,
        };
        let next = if shown {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if *visible != next {
            *visible = next;
        }
    }
}

fn random(mut x: u32) -> f32 {
    x = (x ^ (x >> 16)).wrapping_mul(0x7feb352d);
    x = (x ^ (x >> 15)).wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    (x >> 8) as f32 / 16777216.
}
// Only centers are evaluated on CPU for exact back-to-front alpha ordering.
// Vertex motion, billboard orientation, size, color and turbulence remain on GPU.
fn center(kind: u32, i: u32, t: f32) -> Vec3 {
    let tau = std::f32::consts::TAU;
    match kind {
        0 if i < 54 => {
            let seed = random(i + 1);
            let age = (t * 0.75 + seed).fract();
            let angle = random(i + 91) * tau;
            let radius = (1. - age) * 0.35;
            Vec3::new(
                angle.cos() * radius + 0.15 * (t * 2. + seed * 12.).sin(),
                0.12 + age * 1.9,
                angle.sin() * radius,
            )
        }
        0 => {
            let seed = random(i - 54 + 901);
            let age = (t * 0.3 + seed).fract();
            let angle = seed * tau;
            Vec3::new(0.45 * (angle + age * 3.).sin(), age * 3., 0.4 * angle.cos())
        }
        1 if i < 72 => {
            let age = blast_age(t);
            let angle = random(i + 1301) * tau;
            let lift = random(i + 701);
            let radius = age.min(2.5).sqrt() * 1.25;
            Vec3::new(
                angle.cos() * radius * (0.35 + lift * 0.65),
                0.6 + age * (1.4 + lift * 2.6),
                angle.sin() * radius,
            )
        }
        1 => {
            let n = i - 72;
            let age = blast_age(t).min(2.);
            let angle = random(n + 4101) * tau;
            let speed = 1.6 + random(n + 151) * 2.;
            Vec3::new(
                angle.cos() * age * speed,
                (age * (2.5 + random(n + 9) * 2.) - 2.6 * age * age).max(0.04),
                angle.sin() * age * speed,
            )
        }
        2 => {
            let seed = random(i + 2001);
            let age = (t * 1.5 + seed).fract();
            let angle = seed * tau;
            Vec3::new(
                0.75 + angle.cos() * age * 0.7,
                -0.07 + (age * 1.3 - 1.25 * age * age).max(0.),
                1.45 + angle.sin() * age * 0.7,
            )
        }
        4 => {
            let seed = random(i + 3001);
            let age = (t * 0.12 + seed).fract();
            let angle = random(i + 1801) * tau;
            Vec3::new(
                age * 1.1 + (angle + t * 0.3).sin() * (0.15 + age * 0.55),
                0.35 + age * 5.5,
                (angle + age * 2.).cos() * (0.2 + age * 0.55),
            )
        }
        _ => unreachable!(),
    }
}
pub fn sort_particles(t: f32, eye: Vec3, particles: &mut Assets<Particles>) {
    let mut depths = [(0_u32, 0_f32); 108];
    for (_, material) in particles.iter_mut() {
        let kind = material.kind;
        if kind == 3 || (kind == 1 && blast_age(t) > 2.5) {
            continue;
        }
        let count = [78, 108, 45, 36, 52][kind as usize];
        let local_eye = eye - origin(kind as usize);
        for (i, value) in depths[..count].iter_mut().enumerate() {
            *value = (
                i as u32,
                center(kind, i as u32, t).distance_squared(local_eye),
            );
        }
        depths[..count].sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
        for (slot, (id, _)) in depths[..count].iter().enumerate() {
            material.order[slot / 4][slot % 4] = *id;
        }
    }
}
