# Synthetic benchmark report example

**SYNTHETIC DATA ONLY — these are not device measurements or an engine ranking.**

Three generated repetitions per engine use 600 identical 100 ms application intervals. No CPU or GPU measurements are invented.

| Metric | Filament | Godot | Bevy |
|---|---:|---:|---:|
| Average FPS | 10.000 | 10.000 | 10.000 |
| 1% Low FPS | 10.000 | 10.000 | 10.000 |
| Frame Time P50 (ms) | 100.000 | 100.000 | 100.000 |
| Frame Time P95 (ms) | 100.000 | 100.000 | 100.000 |
| Frame Time P99 (ms) | 100.000 | 100.000 | 100.000 |
| CPU Update Time | unavailable | unavailable | unavailable |
| CPU Render Time | unavailable | unavailable | unavailable |
| GPU Frame Time | unavailable | unavailable | unavailable |
| GPU Slowest Pass | unavailable | unavailable | unavailable |
| 3D Resolution | 1280×720 | 1280×720 | 1280×720 |
| MSAA fixture value | 2× | 2× | 2× |

These fixture settings do not describe the engines' real default quality.
Use `python3 benchmark/examples.py /tmp/forest-examples` to generate nine full run folders and the complete CLI report.
The committed `example-*.json/csv` files demonstrate one complete synthetic run.
