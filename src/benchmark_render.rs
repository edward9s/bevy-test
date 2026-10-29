//! Wrap the pinned native diagnostics plugin; gate only its newly registered systems.
//! Resource presence latches the GPU mode for an entire render frame.
use bevy::{
    ecs::schedule::{BoxedCondition, ConditionWithAccess, ScheduleLabel, SystemKey},
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        batching::gpu_preprocessing::GpuPreprocessingSupport,
        diagnostic::{DiagnosticsRecorder, RenderDiagnosticsPlugin, begin_diagnostics_frame},
        render_resource::{CachedPipelineState, PipelineCache, WgpuFeatures},
        renderer::{RenderDevice, RenderGraph, RenderGraphSystems},
    },
};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    time::Instant,
};
pub static ENABLED: AtomicBool = AtomicBool::new(false);
pub static ACTIVE: AtomicBool = AtomicBool::new(false);
pub static PIPELINE_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static PIPELINE_ERROR: AtomicBool = AtomicBool::new(false);
pub static READY: AtomicBool = AtomicBool::new(false);
pub static PREPROCESSING: AtomicU8 = AtomicU8::new(255);
pub static TIMESTAMPS: AtomicU8 = AtomicU8::new(0);
pub static COLLECT: AtomicBool = AtomicBool::new(false);
pub static CLOCK: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
#[derive(Clone, Copy)]
pub struct CpuSample {
    pub at: u64,
    pub prepare: f64,
    pub submit: f64,
}
pub static CPU: Mutex<Vec<CpuSample>> = Mutex::new(Vec::new());
#[derive(Resource)]
struct Parked(DiagnosticsRecorder);
#[derive(Resource, Default)]
struct Timers {
    prepare: Option<Instant>,
    submit: Option<Instant>,
    prepare_ms: f64,
}
pub struct RuntimeDiagnosticsPlugin;
fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}
fn present(recorder: Option<Res<DiagnosticsRecorder>>) -> bool {
    recorder.is_some()
}
fn keys(world: &mut World, label: impl ScheduleLabel) -> Vec<SystemKey> {
    world
        .get_resource::<Schedules>()
        .and_then(|s| s.get(label))
        .map(|s| s.graph().systems.iter().map(|(k, _, _)| k).collect())
        .unwrap_or_default()
}
fn gate(world: &mut World, label: impl ScheduleLabel, before: &[SystemKey], render: bool) -> usize {
    let mut schedules = world.resource_mut::<Schedules>();
    let schedule = schedules
        .get_mut(label)
        .expect("diagnostic schedule missing");
    let added: Vec<_> = schedule
        .graph()
        .systems
        .iter()
        .map(|(k, _, _)| k)
        .filter(|k| !before.contains(k))
        .collect();
    for key in &added {
        let condition: BoxedCondition = if render {
            Box::new(IntoSystem::into_system(present))
        } else {
            Box::new(IntoSystem::into_system(enabled))
        };
        schedule
            .graph_mut()
            .systems
            .get_conditions_mut(*key)
            .unwrap()
            .push(ConditionWithAccess::new(condition));
    }
    added.len()
}
impl Plugin for RuntimeDiagnosticsPlugin {
    fn build(&self, app: &mut App) {
        let before = keys(app.world_mut(), PreUpdate);
        RenderDiagnosticsPlugin.build(app);
        assert_eq!(
            gate(app.world_mut(), PreUpdate, &before, false),
            1,
            "pinned diagnostic main system registration changed"
        );
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .init_resource::<Timers>()
                .add_systems(
                    Render,
                    (
                        toggle
                            .after(RenderSystems::ExtractCommands)
                            .before(begin_diagnostics_frame)
                            .before(RenderSystems::PrepareAssets),
                        prepare_start
                            .after(RenderSystems::ExtractCommands)
                            .before(RenderSystems::PrepareAssets),
                        prepare_end
                            .after(RenderSystems::Prepare)
                            .before(RenderSystems::Render),
                        readiness.after(RenderSystems::Cleanup),
                    ),
                )
                .add_systems(
                    RenderGraph,
                    (
                        submit_start
                            .before(RenderGraphSystems::Submit)
                            .after(RenderGraphSystems::Render),
                        submit_end
                            .after(RenderGraphSystems::Submit)
                            .before(RenderGraphSystems::Finish),
                    ),
                );
        }
    }
    fn finish(&self, app: &mut App) {
        let (before, graph) = if let Some(render) = app.get_sub_app_mut(RenderApp) {
            (
                keys(render.world_mut(), Render),
                keys(render.world_mut(), RenderGraph),
            )
        } else {
            return;
        };
        RenderDiagnosticsPlugin.finish(app);
        let render = app
            .get_sub_app_mut(RenderApp)
            .expect("RenderApp moved before diagnostics finish");
        assert_eq!(
            gate(render.world_mut(), Render, &before, true)
                + gate(render.world_mut(), RenderGraph, &graph, true),
            3,
            "pinned diagnostic render system registration changed"
        );
    }
}
fn toggle(world: &mut World) {
    if enabled() {
        if let Some(parked) = world.remove_resource::<Parked>() {
            world.insert_resource(parked.0);
        }
    } else if let Some(recorder) = world.remove_resource::<DiagnosticsRecorder>() {
        world.insert_resource(Parked(recorder));
    }
    ACTIVE.store(
        world.contains_resource::<DiagnosticsRecorder>(),
        Ordering::Release,
    );
}
fn readiness(
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    support: Res<GpuPreprocessingSupport>,
    mut last_error_check: Local<Option<Instant>>,
) {
    PIPELINE_COUNT.store(cache.pipelines().count(), Ordering::Release);
    if crate::benchmark::locked()
        && last_error_check.is_none_or(|at| at.elapsed().as_secs_f32() >= 1.)
    {
        PIPELINE_ERROR.store(
            cache
                .pipelines()
                .any(|p| matches!(&p.state, CachedPipelineState::Err(_))),
            Ordering::Release,
        );
        *last_error_check = Some(Instant::now());
    }
    READY.store(
        cache.waiting_pipelines().next().is_none(),
        Ordering::Release,
    );
    PREPROCESSING.store(support.max_supported_mode as u8, Ordering::Release);
    let f = device.features();
    TIMESTAMPS.store(
        u8::from(f.contains(WgpuFeatures::TIMESTAMP_QUERY))
            | (u8::from(f.contains(WgpuFeatures::TIMESTAMP_QUERY_INSIDE_PASSES)) * 2)
            | (u8::from(f.contains(WgpuFeatures::TIMESTAMP_QUERY_INSIDE_ENCODERS)) * 4),
        Ordering::Release,
    );
}
fn prepare_start(mut t: ResMut<Timers>) {
    t.prepare = COLLECT.load(Ordering::Acquire).then(Instant::now);
}
fn prepare_end(mut t: ResMut<Timers>) {
    if let Some(start) = t.prepare.take() {
        t.prepare_ms = start.elapsed().as_secs_f64() * 1000.;
    }
}
fn submit_start(mut t: ResMut<Timers>) {
    t.submit = COLLECT.load(Ordering::Acquire).then(Instant::now);
}
fn submit_end(mut t: ResMut<Timers>) {
    if let Some(start) = t.submit.take() {
        let submit_ms = start.elapsed().as_secs_f64() * 1000.;
        let Some(clock) = CLOCK.get() else {
            return;
        };
        let observed_at = clock.elapsed().as_nanos() as u64;
        let mut q = CPU.lock().unwrap();
        if q.len() == 8 {
            q.remove(0);
        }
        q.push(CpuSample {
            at: observed_at,
            prepare: t.prepare_ms,
            submit: submit_ms,
        });
    }
}
