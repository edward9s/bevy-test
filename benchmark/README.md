# Unified forest benchmark — implementation in progress

Schema version `1.0.0`. This branch is a work in progress; no Galaxy Tab S8 measurements or repository-key signing verification are claimed.

Each run directory contains `metadata.json`, `frames.csv`, `metrics.csv`, and `summary.json`.
The metadata and summary carry the same `schema_version`, `run_id`, `preset_id`, `valid`, and `invalid_reason`.
`test_compare.py::fixture()` is an executable **synthetic** schema example. Do not publish its numbers as device results.

## Statistics and clocks

Use a monotonic clock, collect application callback intervals only after a 30 second warmup, then measure at least 60 seconds. Reset camera and animation clock before warmup. Repeat each preset three times independently, restoring baseline settings between runs.

- Mean FPS = 1000 × sample count / sum of intervals in milliseconds.
- Mean frame time = sum / count.
- P50/P95/P99 use nearest rank: sorted intervals at `ceil(p × count) - 1`.
- 1% low FPS = 1000 / mean of the **slowest `ceil(count × .01)` intervals**.
- The first measured timestamp anchors the interval sequence and is not itself a sample.
- Application callback FPS is not presentation FPS. Unmeasured presentation FPS is `null`.
- Store actual CPU/GPU scopes, source API, and units. Never infer whole-frame GPU time from a slowest pass or sum potentially overlapping passes.

`frames.csv` columns:

```csv
frame_index,elapsed_ms,interval_ms,animation_seconds,cpu_scene_ms,cpu_effects_ms,cpu_submit_ms,cpu_wait_ms
```

Unsupported CPU values are empty. Nonempty CPU columns require a matching metric definition in metadata. `metrics.csv` is for asynchronous native observations:

```csv
sample_id,observed_frame,metric,value_ms
```

`observed_frame` identifies the application callback receiving the result, not the GPU execution frame. Deduplicate native samples before writing. Never emit zero as a substitute for unavailable timestamps.

## Comparison CLI

```sh
python3 -m unittest discover -s benchmark -p 'test_*.py'
python3 benchmark/compare.py exported-directory engine2.zip engine3.zip --validate-only
python3 benchmark/compare.py exported-directory engine2.zip engine3.zip --output comparison
```

The tool uses only the Python standard library. It recomputes statistics from raw intervals, checks dimensions and preset settings, rejects malformed data, groups backend / presentation policy / profiling / hardware / scene conditions, and writes `comparison.md` and `comparison.csv`.
Different builds or settings of the same engine refuse aggregation. Unknown actual presentation modes are isolated per engine, preventing unsupported cross-engine rankings. Three engines × three repetitions are needed for a complete comparison. CPU scopes are shown separately. GPU passes keep their names.

## Required physical-device verification

Pending: physical-device validation of Android integration, repository-key signed APK checks, start/cancel/export and restore behavior, background/rotation invalidation, shader/reflection readiness, actual GPU timestamp support, repeated-run memory use, showcase vs Standard overhead, thermal behavior and matched screenshots at 38.52 seconds.

For Bevy's 8–9 FPS, independent resolution/shadow/AA/Bloom/effects tests can identify sensitivity to those workloads. Native render-world timings can separate preparation and submission scopes. These observations alone cannot prove shader ALU, texture bandwidth, occupancy, overdraw, driver stalls or CPU/GPU queue dependencies; AGI/Perfetto or supported vendor counters are still needed. Adreno GPU preprocessing support must be recorded from the actual Bevy adapter, not assumed from model name.

## Bevy Android integration

Use the Android **Benchmark** button (top right): each of eight presets runs three times, progress button cancels, **View results** shows summaries and **Export ZIP** opens SAF. Reference capture freezes only its separate 38.52 s screenshot; Android PixelCopy is asynchronous. A matching settings JSON accompanies the screenshot. Showcase camera/presentation/shadows/Bloom/AA/render scale and virtual clock are restored afterward.

Baseline uses the existing scene: four 2048 shadow cascades at 48 m, native PBR, fog, Bloom, foliage and six effect batches. Actual camera MSAA is recorded from the selected component, following existing adapter HDR/depth checks for 2× or 4×; no quality choice was reduced. No-effects hides water/particles, sets local lights to zero, and skips effect CPU work; foliage wind continues. No-Bloom removes only Bloom. 65% scales each 3D target dimension with native UI preserved.

The native `RenderDiagnosticsPlugin` is registered once before pipelined rendering. A wrapper identifies the exact systems added by this pinned plugin (one main, three render), applies run conditions and asserts the counts, avoiding release-build debug names. It parks/reinstates `DiagnosticsRecorder` at render-frame start; render conditions use latched resource presence, so main-thread requests cannot remove a recorder midway through a render frame. Standard collects no detailed GPU spans. No GPU wait/readback was added to measurement. Pipeline statistics remain disabled. Native diagnostic paths and sample timestamps are retained/deduplicated. A slowest GPU pass is never a whole GPU frame.

CPU scopes are separate: raw effect update, PreUpdate→Last main-world wall envelope, render PrepareAssets→after Prepare wall envelope, and RenderGraph Submit envelope excluding swapchain presentation. These envelopes include scheduling/waits and are not pure busy-time or pure synchronization measurements; pipelined worlds overlap. Asynchronous observations identify receipt frames only. Pass timestamps can arrive after their execution frame; boundary attribution remains approximate.

Adapter name/driver/backend, maximum supported preprocessing mode, actual MSAA, and timestamp feature bits are exported. Bevy 0.20 explicitly disables GPU preprocessing on Adreno 730 and earlier except 720. This explains a path difference, not the measured 8–9 FPS by itself. Preparation/submission scopes and independent A/B results narrow the investigation; AGI/Perfetto is needed for shader/bandwidth/occupancy/overdraw/queue-stall attribution.

Build provenance: `python3 benchmark/build.py` before Cargo; workflows do this automatically. `Cargo.toml`, `Cargo.lock`, toolchain pins, dependency-key inputs and persistent dependency restore/save scripts are unchanged. Java bridge packaging adds DEX without a third-party dependency. Signed release secrets retain their existing names.

Verified: Rust check/Clippy, 9 Rust tests, Android arm64 cargo check, Java/DEX, native diagnostic off/on/off on software Vulkan, eight synthetic exports/restores, and an actual offscreen scene capture. GitHub CI also passed Rust/material rendering/target resize/export checks. Full release APK packaging and all target-device checks remain pending.

Sources: [native diagnostics](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_render/src/diagnostic/mod.rs), [Adreno preprocessing restriction](https://github.com/bevyengine/bevy/blob/v0.20.0/crates/bevy_render/src/batching/gpu_preprocessing.rs).

Formal JSON schemas: `metadata.schema.json`, `summary.schema.json`. The CLI also checks CSV timelines, scope definitions, preset conditions and recomputed statistics. Run `python3 benchmark/examples.py /tmp/forest-examples` for nine clearly marked synthetic runs plus an example Markdown/CSV report.

## Galaxy Tab S8 procedure (all three APKs)

1. Install matching Release APKs built from the three PR branches, with validation, Vulkan queue waits and pipeline statistics disabled. Record APK hashes and build commits. Use the same landscape orientation, screen mode/refresh, brightness and thermal starting condition; close background workloads. Filament: select Vulkan to compare Vulkan results; keep GLES in its own group.
2. Take a Reference screenshot at 38.52 s in each APK at the same native Surface size. Export settings and inspect materials, shadows, foliage, water, particles, exposure and postprocessing differences. Existing native pipelines differ; matching option names do not prove image equality.
3. Run Standard (three repetitions automatically). Do not touch camera or change display settings. Initialization and 30 s warmup precede each 60 s measurement. Let the tablet cool consistently between engine/preset batches. Keep all repetitions, including invalid runs with reasons.
4. Run GPU Diagnostic separately. Export each ZIP through Android's document picker. GPU Diagnostic FPS must remain separate from Standard. Check actual MSAA, Surface/3D sizes, GPU timestamp availability and profiling flags.
5. Run each Bottleneck preset independently (baseline, no shadows, no AA, no Bloom, fixed 65% axes, no effects). Restore to showcase after each group. Compare deltas only within matching backend/build/device/presentation conditions. A/B effects identify sensitivity, not unique hardware causes.
6. Validate and report using `python3 benchmark/compare.py exports/*.zip --output tab-s8`. Unknown actual presentation modes intentionally prevent a formal cross-engine ranking; the report still exposes each engine's observations/scopes.
7. Validate cancellation, background/resume, Surface resize, refresh change and settings changes; affected runs must be invalid. Repeat tests and watch memory/thermal behavior. Measure showcase vs Standard overhead and GPU Diagnostic overhead separately. These checks are pending until done on hardware.

No Galaxy Tab S8 is attached to this environment. No tablet FPS, screenshot equivalence or overhead acceptance is claimed.

The report also provides a separate within-engine A/B sensitivity table. It only allows the selected setting to differ and keeps GPU Diagnostic deltas explicitly labeled profiling overhead; it does not turn those deltas into a unique hardware diagnosis.

APK contents are checked with `python3 benchmark/check_apk.py ENGINE path.apk --commit SHA`: arm64 ELF, DEX integrity, runtime bridge/scripts, required scene assets and baked commit. This supplements the existing signer/zip alignment checks; it does not replace them.

Before warmup, each real run saves an empty invalid snapshot marked in progress. On next launch, a complete snapshot left by a process interruption becomes an invalid result with its reason; partially written files remain excluded and are identified as incomplete in the results viewer. No journal/disk updates occur during measurement. Reference captures are separate and create no benchmark run journal.

Committed `fixtures/example-metadata.json`, `example-frames.csv`, `example-metrics.csv`, `example-summary.json`, and [example-report.md](fixtures/example-report.md) are **synthetic fixtures**, not device scores. `examples.py` creates complete directly loadable run folders. Use one latest full ZIP per APK; overlapping exports are rejected for duplicate run IDs.

GPU Slowest Pass selects the largest median of per-run mean observations among named spans. Native spans may be nested/inclusive; they are not automatically leaf passes or whole GPU duration. Official results come from exported summaries. Showcase HUD counters in Godot/Bevy pause during measurement.

Physical display mode pixels and rotation are captured through Android's Display API, separately from the application Surface and 3D target. Missing physical display mode information is explicitly marked in the report.

Verification completed before the final pipeline-count guard: 9 Rust tests, Clippy, Android arm64 cargo check and Release library build, manually assembled unsigned APK ABI/DEX/JNI/provenance checks, plus software-Vulkan native diagnostics off/on/off and eight synthetic export/restore presets. Pipeline count changes now reset warmup or invalidate measurement, including pipelines that compile entirely between readiness checks. Latest revision checks are recorded by PR CI. Repository-key signing and target-device acceptance remain pending.

RenderStartup device acquisition increments a generation counter; a GPU device reset during an applied benchmark invalidates the run, independently of shader-cache count changes.
