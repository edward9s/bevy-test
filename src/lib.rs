#[cfg(target_os = "android")]
mod android;
mod effects;
mod materials;
mod reflection;
mod resolution;
mod scene;

use bevy::{
    anti_alias::fxaa::{Fxaa, Sensitivity},
    app::{AppExit, ScheduleRunnerPlugin},
    camera::{Exposure, Hdr, RenderTarget},
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    light::{
        CascadeShadowConfig, DirectionalLightShadowMap, EnvironmentMapLight, GlobalAmbientLight,
        ShadowFilteringMethod, Skybox,
    },
    pbr::{DistanceFog, FogFalloff},
    post_process::bloom::Bloom,
    prelude::*,
    render::{
        RenderPlugin,
        diagnostic::RenderDiagnosticsPlugin,
        render_resource::WgpuFeatures,
        settings::{RenderCreation, WgpuSettings},
    },
    window::{PresentMode, PrimaryWindow, WindowResolution},
};
use materials::{Foliage, Particles, Water};

// Match the reference renderer at its fixed exposure.
const LIGHT_SCALE: f32 = std::f32::consts::PI;

#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    distance: f32,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.24,
            pitch: 0.12,
            distance: 13.,
        }
    }
}
impl Orbit {
    fn transform(&self) -> Transform {
        let horizontal = self.distance * self.pitch.cos();
        let target = Vec3::new(0., 1.6, -3.);
        Transform::from_translation(
            target
                + Vec3::new(
                    horizontal * self.yaw.sin(),
                    self.distance * self.pitch.sin(),
                    horizontal * self.yaw.cos(),
                ),
        )
        .looking_at(target, Vec3::Y)
    }
}
#[derive(Component)]
struct OrbitCamera;
#[derive(Component)]
struct Stats;
#[derive(Resource, Default)]
struct Measure {
    elapsed: f32,
    frames: u32,
    effect_us: f32,
    vsync: bool,
    profiling: bool,
}
#[derive(Resource)]
struct Capture {
    path: Option<String>,
    check_resize: bool,
    resize_step: u32,
    seconds: f32,
    captured: bool,
    target: Option<Handle<Image>>,
}

#[cfg(target_os = "android")]
#[bevy_main]
fn main() {
    android::install_panic_log();
    // Winit's event loop and Bevy's logger are process-wide. Once the native
    // runner has finished, a launcher restart must get a fresh process.
    let exit = run();
    std::process::exit(if exit.is_success() { 0 } else { 1 });
}

pub fn run() -> AppExit {
    let profiling =
        option_env!("BEVY_PROFILE") == Some("1") || std::env::args().any(|arg| arg == "--profile");
    let path = std::env::args().find_map(|arg| arg.strip_prefix("--capture=").map(str::to_owned));
    let check_resize = std::env::args().any(|arg| arg == "--check-resize");
    assert!(
        !check_resize || path.is_some(),
        "--check-resize requires --capture"
    );
    if let Some(path) = &path
        && let Some(parent) = std::path::Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).expect("Cannot create capture directory");
    }
    let headless = path.is_some();
    let mut plugins = DefaultPlugins
        .build()
        .set(WindowPlugin {
            primary_window: if headless {
                None
            } else {
                Some(Window {
                    title: "Bevy Forest Test".into(),
                    resolution: WindowResolution::new(1280, 720),
                    present_mode: PresentMode::AutoNoVsync,
                    ..default()
                })
            },
            exit_condition: if headless {
                bevy::window::ExitCondition::DontExit
            } else {
                bevy::window::ExitCondition::OnAllClosed
            },
            ..default()
        })
        .set(RenderPlugin {
            render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                features: WgpuFeatures::empty(),
                // Timings are useful; per-draw pipeline statistics add profiling overhead.
                disabled_features: Some(WgpuFeatures::PIPELINE_STATISTICS_QUERY),
                ..default()
            })),
            ..default()
        });
    if headless {
        plugins = plugins.disable::<bevy::winit::WinitPlugin>();
    }
    let mut app = App::new();
    app.insert_resource(Orbit::default())
        .insert_resource(Measure {
            profiling,
            ..default()
        })
        .insert_resource(Capture {
            path,
            check_resize,
            resize_step: 0,
            seconds: 0.,
            captured: false,
            target: None,
        })
        .insert_resource(GlobalAmbientLight {
            brightness: 0.,
            ..default()
        })
        .insert_resource(DirectionalLightShadowMap { size: 1024 })
        .add_plugins(plugins)
        .add_plugins(reflection::ReflectionPlugin)
        .add_plugins((
            MaterialPlugin::<Foliage>::default(),
            MaterialPlugin::<Water>::default(),
            MaterialPlugin::<Particles>::default(),
        ))
        .add_systems(
            Startup,
            (
                resolution::setup.before(setup),
                setup,
                scene::load,
                effects::setup,
            ),
        )
        .add_systems(
            Update,
            (
                scene::spawn,
                orbit,
                animate,
                controls.before(stats),
                resolution::update.before(stats),
                stats,
                capture,
            ),
        )
        .add_systems(
            PostUpdate,
            resolution::sync_projection.before(bevy::camera::CameraUpdateSystems),
        );
    if profiling {
        app.add_plugins(RenderDiagnosticsPlugin);
    }
    if headless {
        app.add_plugins(ScheduleRunnerPlugin::run_loop(
            std::time::Duration::from_millis(16),
        ));
    }
    app.run()
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    orbit: Res<Orbit>,
    resolution: Res<resolution::Resolution>,
    capture: Res<Capture>,
    mut clock: ResMut<Time<Virtual>>,
) {
    if capture.path.is_some() {
        // A deterministic instant with all six effects visible, including lightning.
        clock.advance_by(std::time::Duration::from_secs_f64(38.52));
        clock.pause();
    }
    commands
        .spawn((
            Camera3d::default(),
            bevy::camera::ShadowLodOrigin,
            bevy::camera::visibility::RenderLayers::from_layers(&[0, 1, 2]),
            RenderTarget::Image(resolution.image.clone().into()),
            Hdr,
            OrbitCamera,
            orbit.transform(),
            Projection::Perspective(PerspectiveProjection {
                fov: 52_f32.to_radians(),
                near: 0.1,
                far: 100.,
                ..default()
            }),
            Msaa::Off,
            Exposure {
                ev100: (1.0_f32 / (1.2 * 0.65)).log2(),
            },
            Tonemapping::AcesFitted,
            Bloom {
                intensity: 0.065,
                ..default()
            },
            Skybox {
                image: Some(scene::image(&server, "sky", false)),
                brightness: 0.5,
                ..default()
            },
            EnvironmentMapLight {
                diffuse_map: scene::image(&server, "diffuse", false),
                specular_map: scene::image(&server, "specular", false),
                intensity: 0.18 * LIGHT_SCALE,
                ..default()
            },
            DistanceFog {
                color: Color::srgb(0.36, 0.43, 0.44),
                falloff: FogFalloff::Exponential { density: 0.004 },
                ..default()
            },
        ))
        .insert((
            Camera {
                order: -1,
                ..default()
            },
            Fxaa {
                edge_threshold: Sensitivity::Medium,
                edge_threshold_min: Sensitivity::Medium,
                ..default()
            },
            ShadowFilteringMethod::Hardware2x2,
        ));
    commands.spawn((
        DirectionalLight {
            color: Color::linear_rgb(1., 0.88, 0.7),
            illuminance: 1.1 * LIGHT_SCALE,
            shadow_maps_enabled: true,
            shadow_depth_bias: 0.03,
            shadow_normal_bias: 0.35,
            ..default()
        },
        Transform::IDENTITY.looking_to(Vec3::new(-0.32, -0.82, -0.47), Vec3::Y),
        CascadeShadowConfig {
            bounds: vec![18., 40.],
            minimum_distance: 0.1,
            overlap_proportion: 0.15,
        },
    ));
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                top: px(12),
                padding: UiRect::all(px(8)),
                ..default()
            },
            BackgroundColor(Color::srgba(0., 0., 0., 0.55)),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Loading forest…"),
                TextFont::from_font_size(20.),
                TextColor(Color::WHITE),
                Stats,
            ));
        });
}

fn orbit(
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    buttons: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut orbit: ResMut<Orbit>,
    mut camera: Single<&mut Transform, With<OrbitCamera>>,
) {
    let size = Vec2::new(window.width(), window.height());
    if size.min_element() <= 0. {
        return;
    }
    let mut delta = Vec2::ZERO;
    // Android produces real touch events; do not also apply synthesized mouse input.
    let fingers: Vec<_> = touches.iter().collect();
    if fingers.len() == 1 {
        delta = fingers[0].delta();
    } else if fingers.len() == 2 {
        let old = fingers[0]
            .previous_position()
            .distance(fingers[1].previous_position());
        let current = fingers[0].position().distance(fingers[1].position());
        if old > 1. && current > 1. {
            orbit.distance = (orbit.distance * old / current).clamp(4., 24.);
        }
    } else if buttons.pressed(MouseButton::Left) {
        delta = motion.delta;
    }
    orbit.yaw -= delta.x / size.x * 4.;
    orbit.pitch = (orbit.pitch + delta.y / size.y * 2.).clamp(0.03, 0.85);
    orbit.distance = (orbit.distance - scroll.delta.y * 0.7).clamp(4., 24.);
    if orbit.is_changed() {
        **camera = orbit.transform();
    }
}
fn animate(
    time: Res<Time>,
    lights: Query<(&effects::EffectLight, &mut PointLight)>,
    batches: Query<(&effects::EffectBatch, &mut Visibility)>,
    mut measure: ResMut<Measure>,
    mut particles: ResMut<Assets<Particles>>,
    camera: Single<&Transform, With<OrbitCamera>>,
) {
    let start = std::time::Instant::now();
    effects::sort_particles(
        time.elapsed_secs_wrapped(),
        camera.translation,
        &mut particles,
    );
    effects::animate(time, lights, batches);
    measure.effect_us = measure.effect_us * 0.9 + start.elapsed().as_secs_f32() * 1_000_000. * 0.1;
}
fn controls(
    mut measure: ResMut<Measure>,
    mut sun: Single<&mut DirectionalLight>,
    buttons: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Option<Single<&mut Window, With<PrimaryWindow>>>,
) {
    let mut window = window;
    if let Some(window) = window.as_mut() {
        let tap = touches
            .iter_just_pressed()
            .map(|finger| finger.position())
            .next()
            .or_else(|| {
                (touches.iter().next().is_none() && buttons.just_pressed(MouseButton::Left))
                    .then(|| window.cursor_position())
                    .flatten()
            });
        if let Some(p) = tap.filter(|p| p.x < 900. && p.y < 150.) {
            if p.y >= 90. {
                sun.shadow_maps_enabled = !sun.shadow_maps_enabled;
            } else if p.y < 50. {
                window.present_mode = if window.present_mode == PresentMode::AutoNoVsync {
                    PresentMode::AutoVsync
                } else {
                    PresentMode::AutoNoVsync
                };
                measure.vsync = window.present_mode == PresentMode::AutoVsync;
            }
        }
    }
}
fn stats(
    time: Res<Time<Real>>,
    forest: Res<scene::Forest>,
    mut measure: ResMut<Measure>,
    mut text: Single<&mut Text, With<Stats>>,
    diagnostics: Res<bevy::diagnostic::DiagnosticsStore>,
    resolution: Res<resolution::Resolution>,
    sun: Single<&DirectionalLight>,
) {
    measure.elapsed += time.delta_secs();
    measure.frames += 1;
    if measure.elapsed >= 0.5 && forest.ready {
        let fps = measure.frames as f32 / measure.elapsed;
        let slowest = diagnostics
            .iter()
            .filter(|d| d.path().as_str().ends_with("/elapsed_gpu"))
            .filter_map(|d| {
                let m = d.measurement()?;
                (m.time.elapsed().as_secs_f32() < 2.).then_some((d.path(), m.value))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        let gpu = slowest.map_or_else(
            || {
                if measure.profiling {
                    "GPU: unavailable"
                } else {
                    "GPU profiling off"
                }
                .to_owned()
            },
            |(path, ms)| {
                format!(
                    "GPU slowest: {ms:.1} ms {}",
                    path.as_str()
                        .trim_start_matches("render/")
                        .trim_end_matches("/elapsed_gpu")
                )
            },
        );
        text.0 = format!(
            "FPS: {fps:.0} | Auto {}x{} {:.0}% | {} (tap)\nFrame: {:.1} ms | Effects CPU: {:.3} ms\n{gpu}\nShadows: {} (tap) | FXAA | {} | {}",
            resolution.size.x,
            resolution.size.y,
            resolution.size.x as f32 / resolution.output.x as f32 * 100.,
            if measure.vsync { "VSync" } else { "Uncapped" },
            1000. / fps,
            measure.effect_us / 1000.,
            if sun.shadow_maps_enabled {
                "2 x 1024"
            } else {
                "Off"
            },
            forest.texture_format,
            if resolution.ready {
                "Ready"
            } else {
                "Warming reflection"
            },
        );
        measure.elapsed = 0.;
        measure.frames = 0;
    }
}
fn capture(
    mut commands: Commands,
    time: Res<Time<Real>>,
    forest: Res<scene::Forest>,
    mut capture: ResMut<Capture>,
    reflection: Res<reflection::Reflection>,
) {
    if capture.path.is_none() || !forest.ready || !reflection.is_ready() {
        return;
    }
    capture.seconds += time.delta_secs();
    if capture.check_resize && capture.resize_step < 3 {
        if capture.seconds >= 0.5 {
            capture.resize_step += 1;
            capture.seconds = 0.;
        }
        return;
    }
    if capture.seconds >= 2. && !capture.captured {
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::image(
                capture.target.clone().expect("Capture target missing"),
            ))
            .observe(bevy::render::view::screenshot::save_to_disk(
                capture.path.clone().unwrap(),
            ))
            .observe(capture_exit);
        capture.captured = true;
    }
}
fn capture_exit(
    _event: On<bevy::render::view::screenshot::ScreenshotCaptured>,
    mut exit: MessageWriter<AppExit>,
) {
    exit.write(AppExit::Success);
}
