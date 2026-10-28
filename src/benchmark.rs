//! Reversible Android benchmark controller. Measurement buffers remain in memory.
use crate::{
    Measure, Orbit, OrbitCamera, benchmark_build,
    benchmark_protocol::{Frame, Phase, Run},
    benchmark_render as render,
    resolution::Resolution,
};
use bevy::{
    camera::visibility::RenderLayers,
    post_process::bloom::Bloom,
    prelude::*,
    render::renderer::RenderAdapterInfo,
    window::{PresentMode, PrimaryWindow},
};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicI32, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
static COMMAND: AtomicI32 = AtomicI32::new(0);
static STATUS: AtomicI32 = AtomicI32::new(0);
static REPETITION: AtomicI32 = AtomicI32::new(1);
static LOCKED: AtomicBool = AtomicBool::new(false);
static NO_EFFECTS: AtomicBool = AtomicBool::new(false);
pub fn locked() -> bool {
    LOCKED.load(Ordering::Acquire)
}
pub fn effects_disabled() -> bool {
    NO_EFFECTS.load(Ordering::Acquire)
}
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_benchmark_Bridge_command(
    _: *mut std::ffi::c_void,
    _: *mut std::ffi::c_void,
    value: i32,
) {
    COMMAND.store(value, Ordering::Release);
}
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_benchmark_Bridge_status(
    _: *mut std::ffi::c_void,
    _: *mut std::ffi::c_void,
) -> i32 {
    STATUS.load(Ordering::Acquire)
}
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_benchmark_Bridge_repetition(
    _: *mut std::ffi::c_void,
    _: *mut std::ffi::c_void,
) -> i32 {
    REPETITION.load(Ordering::Acquire)
}
const PRESETS: [&str; 8] = [
    "standard",
    "gpu",
    "baseline",
    "no_shadows",
    "no_aa",
    "no_bloom",
    "scale_65",
    "no_effects",
];
struct Saved {
    camera: Entity,
    window: Option<Entity>,
    sun: Entity,
    orbit: Orbit,
    resolution: Resolution,
    clock: Time<Virtual>,
    msaa: Msaa,
    bloom: Option<Bloom>,
    layers: RenderLayers,
    shadows: bool,
    present: Option<PresentMode>,
    profiling: bool,
}
struct Metric {
    id: u64,
    frame: usize,
    name: std::borrow::Cow<'static, str>,
    ms: f64,
}
#[derive(Resource)]
struct State {
    saved: Saved,
    run: Run,
    preset: usize,
    reference: bool,
    applied: bool,
    drawn: u32,
    animation_origin: u64,
    main_start: u64,
    id: String,
    started: u128,
    device: String,
    thermal: String,
    refresh: String,
    test_data: bool,
    surface: UVec2,
    render_size: UVec2,
    metrics: Vec<Metric>,
    definitions: BTreeMap<String, String>,
    seen: HashMap<String, bevy::platform::time::Instant>,
    settings: String,
    poll: u64,
}
pub struct BenchmarkPlugin;
impl Plugin for BenchmarkPlugin {
    fn build(&self, app: &mut App) {
        render::CLOCK.get_or_init(Instant::now);
        render::CPU.lock().unwrap().reserve(8);
        app.add_systems(PreUpdate, control)
            .add_systems(Last, (collect, check).chain());
        if app.world().resource::<crate::Capture>().check_benchmark {
            app.insert_resource(Check {
                stage: 0,
                original: render::ENABLED.load(Ordering::Acquire),
            });
        }
    }
}
fn now() -> u64 {
    render::CLOCK.get().unwrap().elapsed().as_nanos() as u64
}
fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
fn root() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        return bevy::android::ANDROID_APP
            .get()
            .expect("AndroidApp missing")
            .internal_data_path()
            .expect("Android internal storage missing")
            .join("benchmarks");
    }
    #[cfg(not(target_os = "android"))]
    {
        PathBuf::from("build/benchmark-output")
    }
}
fn read_device() -> (String, String, String) {
    #[cfg(target_os = "android")]
    {
        let base = root().parent().unwrap().to_path_buf();
        (
            std::fs::read_to_string(base.join("benchmark-device.json"))
                .expect("Device metadata missing"),
            std::fs::read_to_string(base.join("benchmark-thermal.json"))
                .expect("Thermal metadata missing"),
            std::fs::read_to_string(base.join("benchmark-refresh.json"))
                .expect("Refresh metadata missing"),
        )
    }
    #[cfg(not(target_os = "android"))]
    {
        (
            r#"{"brand":null,"model":null,"android":null,"fingerprint":null}"#.into(),
            "null".into(),
            "null".into(),
        )
    }
}
fn capture_saved(world: &mut World) -> Saved {
    let camera = world
        .query_filtered::<Entity, With<OrbitCamera>>()
        .single(world)
        .expect("Benchmark camera missing");
    let sun = world
        .query_filtered::<Entity, With<DirectionalLight>>()
        .single(world)
        .expect("Benchmark sun missing");
    let window = world
        .query_filtered::<Entity, With<PrimaryWindow>>()
        .iter(world)
        .next();
    Saved {
        camera,
        window,
        sun,
        orbit: world.resource::<Orbit>().clone(),
        resolution: world.resource::<Resolution>().clone(),
        clock: *world.resource::<Time<Virtual>>(),
        msaa: *world.get::<Msaa>(camera).unwrap(),
        bloom: world.get::<Bloom>(camera).cloned(),
        layers: world.get::<RenderLayers>(camera).unwrap().clone(),
        shadows: world
            .get::<DirectionalLight>(sun)
            .unwrap()
            .shadow_maps_enabled,
        present: window.map(|e| world.get::<Window>(e).unwrap().present_mode),
        profiling: world.resource::<Measure>().profiling,
    }
}
fn start(world: &mut World, preset: usize, reference: bool) -> State {
    let saved = capture_saved(world);
    let surface = saved.resolution.output;
    let render_size = saved.resolution.size;
    let (device, thermal, refresh) = read_device();
    State {
        saved,
        run: Run::new(),
        preset,
        reference,
        applied: false,
        drawn: 0,
        animation_origin: now(),
        main_start: now(),
        id: format!("bevy-{}-{}", unix_ms(), now()),
        started: unix_ms(),
        device,
        thermal,
        refresh,
        test_data: false,
        surface,
        render_size,
        metrics: Vec::with_capacity(180000),
        definitions: BTreeMap::new(),
        seen: HashMap::new(),
        settings: String::new(),
        poll: 0,
    }
}
fn apply(world: &mut World, s: &mut State) {
    *world.resource_mut::<Orbit>() = Orbit::default();
    let transform = world.resource::<Orbit>().transform();
    *world.get_mut::<Transform>(s.saved.camera).unwrap() = transform;
    {
        let mut resolution = world.resource_mut::<Resolution>();
        resolution.fixed = true;
        resolution.benchmark_scale = if s.preset == 6 { 0.65 } else { 1. };
    }
    world
        .get_mut::<DirectionalLight>(s.saved.sun)
        .unwrap()
        .shadow_maps_enabled = s.preset != 3;
    *world.get_mut::<Msaa>(s.saved.camera).unwrap() = if s.preset == 4 {
        Msaa::Off
    } else {
        s.saved.msaa
    };
    if s.preset == 5 {
        world.entity_mut(s.saved.camera).remove::<Bloom>();
    } else if let Some(bloom) = s.saved.bloom.clone() {
        world.entity_mut(s.saved.camera).insert(bloom);
    }
    world.entity_mut(s.saved.camera).insert(if s.preset == 7 {
        RenderLayers::layer(0)
    } else {
        s.saved.layers.clone()
    });
    if let Some(e) = s.saved.window {
        world.get_mut::<Window>(e).unwrap().present_mode = PresentMode::AutoNoVsync;
    }
    world.resource_mut::<Measure>().profiling = s.preset == 1;
    render::ENABLED.store(s.preset == 1, Ordering::Release);
    render::COLLECT.store(true, Ordering::Release);
    NO_EFFECTS.store(s.preset == 7, Ordering::Release);
    s.applied = true;
    s.drawn = 0;
    s.animation_origin = now();
    s.settings = settings(world, s);
    s.definitions.insert("cpu_effects_ms".into(),definition("cpu","std::time::Instant","animate: particle ordering, CPU visibility and local-light update; excludes GPU vertex animation","bevy.effects.animate"));
    s.definitions.insert("main_schedule".into(),definition("cpu","std::time::Instant","after command/state acquisition in PreUpdate control to Last observation; main-world wall time, excludes render sub-app and may include OS scheduling","bevy.main.schedule"));
    s.definitions.insert("render_prepare".into(),definition("cpu","std::time::Instant","render world after ExtractCommands/before PrepareAssets to after Prepare/before Render; wall envelope includes parallel task waits","bevy.render.prepare"));
    s.definitions.insert("render_submit".into(),definition("cpu","std::time::Instant","RenderGraph Submit set envelope only; excludes swapchain presentation and subsequent screenshot/readback submission","bevy.render.submit"));
}
fn restore(world: &mut World, s: State) {
    *world.resource_mut::<Orbit>() = s.saved.orbit;
    let transform = world.resource::<Orbit>().transform();
    *world.get_mut::<Transform>(s.saved.camera).unwrap() = transform;
    let mut resolution = s.saved.resolution;
    resolution.dirty = true;
    *world.resource_mut::<Resolution>() = resolution;
    *world.resource_mut::<Time<Virtual>>() = s.saved.clock;
    *world.resource_mut::<Time>() = s.saved.clock.as_generic();
    *world.get_mut::<Msaa>(s.saved.camera).unwrap() = s.saved.msaa;
    world.entity_mut(s.saved.camera).remove::<Bloom>();
    if let Some(bloom) = s.saved.bloom {
        world.entity_mut(s.saved.camera).insert(bloom);
    }
    world.entity_mut(s.saved.camera).insert(s.saved.layers);
    world
        .get_mut::<DirectionalLight>(s.saved.sun)
        .unwrap()
        .shadow_maps_enabled = s.saved.shadows;
    if let (Some(e), Some(present)) = (s.saved.window, s.saved.present) {
        world.get_mut::<Window>(e).unwrap().present_mode = present;
    }
    world.resource_mut::<Measure>().profiling = s.saved.profiling;
    render::ENABLED.store(s.saved.profiling, Ordering::Release);
    render::COLLECT.store(false, Ordering::Release);
    NO_EFFECTS.store(false, Ordering::Release);
    LOCKED.store(false, Ordering::Release);
    render::CPU.lock().unwrap().clear();
}
fn set_clock(world: &mut World, elapsed: f64, reference: bool) {
    let real_delta = world
        .resource::<Time<Real>>()
        .delta()
        .as_secs_f64()
        .min(elapsed);
    let mut clock = Time::<Virtual>::default();
    clock.advance_by(Duration::from_secs_f64((elapsed - real_delta).max(0.)));
    clock.advance_by(Duration::from_secs_f64(real_delta));
    if reference {
        clock.pause();
    }
    *world.resource_mut::<Time<Virtual>>() = clock;
    *world.resource_mut::<Time>() = clock.as_generic();
}
fn control(world: &mut World) {
    let command = COMMAND.swap(0, Ordering::AcqRel);
    let Some(mut s) = world.remove_resource::<State>() else {
        if (1..=9).contains(&command) {
            let physical = world
                .query_filtered::<&Window, With<PrimaryWindow>>()
                .iter(world)
                .next()
                .map(|w| w.resolution.physical_size());
            if physical.is_some_and(|size| {
                size.min_element() == 0 || size != world.resource::<Resolution>().output
            }) {
                COMMAND.store(command, Ordering::Release);
                STATUS.store(7000, Ordering::Release);
                return;
            }
            let mut s = start(
                world,
                if command == 9 {
                    0
                } else {
                    command as usize - 1
                },
                command == 9,
            );
            REPETITION.store(1, Ordering::Release);
            journal(world, &mut s);
            LOCKED.store(true, Ordering::Release);
            STATUS.store(7000, Ordering::Release);
            world.insert_resource(s);
        }
        return;
    };
    s.main_start = now();
    if render::PIPELINE_ERROR.load(Ordering::Acquire) {
        s.run
            .stop("render pipeline compilation failed; see native engine log");
    }
    if command == 10 && s.reference {
        restore(world, s);
        STATUS.store(0, Ordering::Release);
        return;
    }
    if command == 90 || command == 98 || command == 99 {
        s.run.stop(if command == 90 {
            "cancelled by user"
        } else if command == 98 {
            "display refresh rate changed"
        } else {
            "application paused"
        });
    }
    if let Some(e) = s.saved.window {
        let w = world.get::<Window>(e).unwrap();
        if w.resolution.physical_size() != s.surface {
            s.run.stop("Surface dimensions changed");
        }
        if !w.focused {
            s.run.stop("window focus lost");
        }
    }
    if s.applied {
        let elapsed = if s.reference {
            38.52
        } else {
            (now() - s.animation_origin) as f64 * 1e-9
        };
        set_clock(world, elapsed, s.reference);
        if now() >= s.poll {
            s.poll = now() + 1_000_000_000;
            if settings(world, &s) != s.settings {
                s.run.stop("benchmark settings changed");
            }
        }
    }
    world.insert_resource(s);
}
fn collect(world: &mut World) {
    let Some(mut s) = world.remove_resource::<State>() else {
        return;
    };
    let ready = world.resource::<Resolution>().ready && render::READY.load(Ordering::Acquire);
    if !s.applied && ready && s.run.phase != Phase::Finished {
        apply(world, &mut s);
        world.insert_resource(s);
        return;
    }
    s.drawn += 1;
    if s.applied {
        s.render_size = world.resource::<Resolution>().size;
        let scale = if s.preset == 6 { 0.65 } else { 1. };
        let expected = (s.surface.as_vec2() * scale).round().as_uvec2();
        let mode_ready = render::ACTIVE.load(Ordering::Acquire) == (s.preset == 1);
        let ready = ready && s.drawn >= 120 && s.render_size == expected && mode_ready;
        if s.reference && s.run.phase != Phase::Finished {
            if ready && STATUS.load(Ordering::Acquire) != 6000 {
                std::fs::create_dir_all(root()).expect("Cannot create reference directory");
                std::fs::write(root().join("reference-settings.json"), metadata(world, &s))
                    .expect("Cannot save reference settings");
                STATUS.store(6000, Ordering::Release);
            }
            world.insert_resource(s);
            return;
        }
        let animation = world.resource::<Time>().elapsed_secs_f64();
        let effects = world.resource::<Measure>().effect_raw_ms;
        if s.run.sample(
            now(),
            ready,
            Frame {
                animation,
                effects,
                ..Default::default()
            },
        ) {
            s.animation_origin = now();
        }
        if s.run.phase == Phase::Measure && !s.run.frames.is_empty() {
            observe(world, &mut s);
        }
    }
    if s.run.phase == Phase::Finished {
        let valid = s.run.valid;
        let cancelled = s.run.reason.as_deref() == Some("cancelled by user");
        let preset = s.preset;
        (s.device, s.thermal, s.refresh) = read_device();
        if let Err(error) = save(world, &s) {
            error!("Benchmark save failed: {error}");
            restore(world, s);
            STATUS.store(4000, Ordering::Release);
            return;
        }
        restore(world, s);
        if valid && REPETITION.load(Ordering::Acquire) < 3 {
            REPETITION.fetch_add(1, Ordering::AcqRel);
            let mut s = start(world, preset, false);
            journal(world, &mut s);
            LOCKED.store(true, Ordering::Release);
            world.insert_resource(s);
            STATUS.store(7000, Ordering::Release);
        } else {
            STATUS.store(
                if valid {
                    3000
                } else if cancelled {
                    5000
                } else {
                    4000
                },
                Ordering::Release,
            );
        }
        return;
    }
    STATUS.store(
        match s.run.phase {
            Phase::Waiting => 7000,
            Phase::Warmup => 1000 + ((now() - s.run.epoch) / 1_000_000_000) as i32,
            Phase::Measure => 2000 + ((now() - s.run.anchor) / 1_000_000_000) as i32,
            Phase::Finished => 4000,
        },
        Ordering::Release,
    );
    world.insert_resource(s);
}
fn observe(world: &World, s: &mut State) {
    let frame = s.run.frames.len() - 1;
    if s.metrics.len() + 32 >= 180000 {
        s.run.stop("metric capacity exceeded");
        return;
    }
    s.metrics.push(Metric {
        id: frame as u64,
        frame,
        name: "main_schedule".into(),
        ms: (now() - s.main_start) as f64 * 1e-6,
    });
    {
        let mut q = render::CPU.lock().unwrap();
        for (index, m) in q.drain(..).enumerate() {
            if m.at < s.run.anchor {
                continue;
            }
            let id = m.at + index as u64;
            s.metrics.push(Metric {
                id,
                frame,
                name: "render_prepare".into(),
                ms: m.prepare,
            });
            s.metrics.push(Metric {
                id,
                frame,
                name: "render_submit".into(),
                ms: m.submit,
            });
        }
    }
    if s.preset != 1 {
        return;
    }
    for diagnostic in world
        .resource::<bevy::diagnostic::DiagnosticsStore>()
        .iter()
    {
        let path = diagnostic.path().as_str();
        let gpu = path.ends_with("/elapsed_gpu");
        if !gpu && !path.ends_with("/elapsed_cpu") {
            continue;
        }
        let Some(m) = diagnostic.measurement() else {
            continue;
        };
        if !m.value.is_finite()
            || m.value <= 0.
            || m.time.elapsed().as_nanos() as u64 > now() - s.run.anchor
        {
            continue;
        }
        if s.seen.get(path).is_some_and(|time| *time == m.time) {
            continue;
        }
        s.seen.insert(path.into(), m.time);
        s.definitions.entry(path.into()).or_insert_with(||definition(if gpu{"gpu_pass"}else{"cpu"},"RenderDiagnosticsPlugin / DiagnosticsStore; native timestamp sample deduplicated","native path span; publication-time window only, earlier GPU work may arrive here; overlapping spans; observation frame is not execution frame",&format!("bevy.{path}")));
        s.metrics.push(Metric {
            id: s.metrics.len() as u64,
            frame,
            name: path.to_owned().into(),
            ms: m.value,
        });
    }
}
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 32 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn definition(kind: &str, source: &str, scope: &str, id: &str) -> String {
    format!(
        "{{\"unit\":\"ms\",\"kind\":{},\"source\":{},\"scope\":{},\"scope_id\":{}}}",
        quote(kind),
        quote(source),
        quote(scope),
        quote(id)
    )
}
fn settings(world: &World, s: &State) -> String {
    let msaa = world.get::<Msaa>(s.saved.camera).unwrap().samples();
    let shadows = world
        .get::<DirectionalLight>(s.saved.sun)
        .unwrap()
        .shadow_maps_enabled;
    let bloom = world.get::<Bloom>(s.saved.camera).is_some();
    let effects = world
        .get::<RenderLayers>(s.saved.camera)
        .unwrap()
        .intersects(&RenderLayers::from_layers(&[1, 2]));
    format!(
        "{{\"msaa_samples\":{msaa},\"taa\":false,\"fxaa\":false,\"shadows\":{shadows},\"bloom\":{bloom},\"effects\":{effects},\"pipeline\":\"Bevy native PBR\",\"shadow_distance_m\":48,\"shadow_cascades\":4,\"shadow_map_size\":2048,\"ssao\":false,\"fog\":true,\"shadow_filter\":\"Gaussian\",\"tonemap\":\"AcesFitted with existing white normalization\",\"scene_image_format\":\"Rgba8UnormSrgb\",\"texture_format\":{},\"gpu_preprocessing_support\":{},\"timestamp_feature_bits\":{}}}",
        quote(world.resource::<crate::scene::Forest>().texture_format),
        render::PREPROCESSING.load(Ordering::Acquire),
        render::TIMESTAMPS.load(Ordering::Acquire)
    )
}
fn identity(s: &State) -> String {
    format!(
        "\"schema_version\":\"1.0.0\",\"run_id\":{},\"preset_id\":{},\"valid\":{},\"invalid_reason\":{}",
        quote(&s.id),
        quote(PRESETS[s.preset]),
        s.run.valid,
        s.run.reason.as_ref().map_or("null".into(), |x| quote(x))
    )
}
fn metadata(world: &World, s: &State) -> String {
    let adapter = world.resource::<RenderAdapterInfo>();
    let backend = match format!("{:?}", adapter.backend).as_str() {
        "Vulkan" => "Vulkan",
        "Gl" => {
            if cfg!(target_os = "android") {
                "OpenGL ES"
            } else {
                "OpenGL"
            }
        }
        "Metal" => "Metal",
        "Dx12" => "DX12",
        _ => "unavailable",
    };
    let engine = benchmark_build::ENGINE.replace(
        "\"build\": \"release\"",
        if cfg!(debug_assertions) {
            "\"build\": \"debug\""
        } else {
            "\"build\": \"release\""
        },
    );
    let optional = |v: &str| {
        if v.trim().is_empty() {
            "null".into()
        } else {
            quote(v)
        }
    };
    let definitions = s
        .definitions
        .iter()
        .map(|(k, v)| format!("{}:{}", quote(k), v))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{{},\"reference_only\":{},\"reference_animation_seconds\":{},\"test_data\":{},\"repetition\":{},\"started_at\":{},\"warmup_seconds\":30,\"measure_seconds\":60,\"engine\":{engine},\"scene\":{},\"device\":{},\"environment\":{{\"backend\":{},\"surface_size\":[{},{}],\"render_size\":[{},{}],\"dynamic_resolution\":{},\"gpu_profiling\":{},\"vsync_requested\":{},\"presentation_mode\":null,\"refresh_hz\":{},\"refresh_limited_possible\":true,\"gpu\":{},\"driver\":{},\"driver_info\":{},\"thermal_status\":{}}},\"settings\":{},\"metrics\":{{{definitions}}},\"limitations\":[\"Application Last callback intervals, not display presentation FPS.\",\"No verified whole-frame GPU span; native named pass timings may overlap and must not be summed.\",\"CPU main/render scopes are wall envelopes, not pure busy or wait time. Pipelined worlds overlap.\",\"Actual swapchain mode unavailable; Android display refresh is captured by Java. AutoNoVsync may remain refresh limited.\",\"GPU preprocessing support: enum 0=None, 1=PreprocessingOnly, 2=Culling, 255=unavailable; timestamp feature bits 1=query, 2=inside passes, 4=inside encoders.\",\"Readiness requires scene assets, reflection completion, no pending pipelines and 120 settled application frames, then 30 second warmup.\",\"Device metadata/thermal snapshot captured before initialization; no continuous thermal series.\",\"No-effects hides six batches and local lights and skips their CPU updates; vegetation remains animated.\"]}}",
        identity(s),
        s.reference,
        if s.reference { "38.52" } else { "null" },
        s.test_data,
        REPETITION.load(Ordering::Acquire),
        s.started,
        benchmark_build::SCENE,
        s.device,
        quote(backend),
        s.surface.x,
        s.surface.y,
        s.render_size.x,
        s.render_size.y,
        !world.resource::<Resolution>().fixed,
        if s.applied {
            s.preset == 1
        } else {
            s.saved.profiling
        },
        quote(if s.applied { "off" } else { "engine_default" }),
        s.refresh,
        optional(&adapter.name),
        optional(&adapter.driver),
        optional(&adapter.driver_info),
        s.thermal,
        settings(world, s)
    )
}
fn csv(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\"\""))
}
fn save(world: &World, s: &State) -> std::io::Result<()> {
    let path = root().join(&s.id);
    std::fs::create_dir_all(&path)?;
    std::fs::write(path.join(".pending"), "incomplete")?;
    let mut frames = String::from(
        "frame_index,elapsed_ms,interval_ms,animation_seconds,cpu_scene_ms,cpu_effects_ms,cpu_submit_ms,cpu_wait_ms\n",
    );
    for (i, f) in s.run.frames.iter().enumerate() {
        frames.push_str(&format!(
            "{i},{:.9},{:.9},{:.9},,{:.9},,\n",
            f.elapsed, f.interval, f.animation, f.effects
        ));
    }
    let mut metrics = String::from("sample_id,observed_frame,metric,value_ms\n");
    for m in &s.metrics {
        metrics.push_str(&format!(
            "{},{},{},{:.9}\n",
            m.id,
            m.frame,
            csv(&m.name),
            m.ms
        ));
    }
    let stats = if s.run.frames.is_empty() {
        "null".into()
    } else {
        let values: Vec<_> = s.run.frames.iter().map(|f| f.interval).collect();
        format!(
            "{{{}}}",
            crate::benchmark_protocol::statistics(&values)
                .iter()
                .map(|(k, v)| format!("{}:{v}", quote(k)))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    std::fs::write(path.join("metadata.json"), metadata(world, s))?;
    std::fs::write(
        path.join("summary.json"),
        format!(
            "{{{},\"presentation_fps\":null,\"application\":{stats}}}",
            identity(s)
        ),
    )?;
    std::fs::write(path.join("frames.csv"), frames)?;
    std::fs::write(path.join("metrics.csv"), metrics)?;
    std::fs::remove_file(path.join(".pending"))
}

/// Optional renderer functional check. All saved timings are synthetic fixtures.
#[derive(Resource)]
struct Check {
    stage: u8,
    original: bool,
}
pub fn check_done() -> bool {
    CHECK_DONE.load(Ordering::Acquire)
}
static CHECK_DONE: AtomicBool = AtomicBool::new(false);
fn check(world: &mut World) {
    let Some(mut c) = world.remove_resource::<Check>() else {
        return;
    };
    if !world.resource::<Resolution>().ready || !render::READY.load(Ordering::Acquire) {
        world.insert_resource(c);
        return;
    }
    match c.stage {
        0 => {
            render::ENABLED.store(false, Ordering::Release);
            c.stage = 1;
        }
        1 => {
            if !render::ACTIVE.load(Ordering::Acquire) {
                render::ENABLED.store(true, Ordering::Release);
                c.stage = 2;
            }
        }
        2 => {
            if render::ACTIVE.load(Ordering::Acquire) {
                render::ENABLED.store(false, Ordering::Release);
                c.stage = 3;
            }
        }
        3 => {
            if !render::ACTIVE.load(Ordering::Acquire) {
                for preset in 0..8 {
                    let mut s = start(world, preset, false);
                    let saved_size = s.saved.resolution.size;
                    let original_camera =
                        world.get::<Transform>(s.saved.camera).unwrap().translation;
                    apply(world, &mut s);
                    s.render_size = (s.surface.as_vec2() * if preset == 6 { 0.65 } else { 1. })
                        .round()
                        .as_uvec2();
                    s.test_data = true;
                    assert_eq!(
                        world
                            .get::<DirectionalLight>(s.saved.sun)
                            .unwrap()
                            .shadow_maps_enabled,
                        preset != 3
                    );
                    assert_eq!(
                        world.get::<Msaa>(s.saved.camera).unwrap().samples(),
                        if preset == 4 {
                            1
                        } else {
                            s.saved.msaa.samples()
                        }
                    );
                    s.run.sample(100, true, Frame::default());
                    s.run.sample(30_000_000_100, true, Frame::default());
                    for i in 1..=600 {
                        s.run.sample(
                            30_000_000_100 + i * 100_000_000,
                            true,
                            Frame {
                                animation: 30. + i as f64 * 0.1,
                                effects: 0.1,
                                ..Default::default()
                            },
                        );
                    }
                    assert!(s.run.valid);
                    save(world, &s).expect("Synthetic export failed");
                    let camera = s.saved.camera;
                    restore(world, s);
                    assert_eq!(world.resource::<Resolution>().size, saved_size);
                    assert_eq!(
                        world.get::<Transform>(camera).unwrap().translation,
                        original_camera
                    );
                }
                render::ENABLED.store(c.original, Ordering::Release);
                info!(
                    "Benchmark check passed: native diagnostics off/on/off; eight SYNTHETIC presets, export and restore"
                );
                CHECK_DONE.store(true, Ordering::Release);
                return;
            }
        }
        _ => unreachable!(),
    }
    world.insert_resource(c);
}

fn journal(world: &World, s: &mut State) {
    if s.reference {
        return;
    }
    s.run.reason = Some("process interrupted before completion".into());
    let result = save(world, s)
        .and_then(|()| std::fs::write(root().join(&s.id).join(".pending"), "in progress"));
    s.run.reason = None;
    if let Err(error) = result {
        error!("Cannot journal benchmark: {error}");
        s.run.stop("cannot save benchmark initialization record");
    }
}
