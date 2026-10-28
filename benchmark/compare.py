#!/usr/bin/env python3
"""Validate forest benchmark exports and report medians of independent runs (stdlib only)."""
import argparse
import csv
import io
import json
import math
from pathlib import Path
import statistics
import zipfile

VERSION = '1.0.0'
PRESETS = ('standard', 'gpu', 'baseline', 'no_shadows', 'no_aa', 'no_bloom', 'scale_65', 'no_effects')
FRAMES = ('frame_index', 'elapsed_ms', 'interval_ms', 'animation_seconds', 'cpu_scene_ms', 'cpu_effects_ms', 'cpu_submit_ms', 'cpu_wait_ms')
METRICS = ('sample_id', 'observed_frame', 'metric', 'value_ms')
ENGINES = ('Filament', 'Godot', 'Bevy')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def number(x, minimum=0):
    return type(x) in (int, float) and math.isfinite(x) and x >= minimum


def integer(x, minimum=0):
    return number(x, minimum) and int(x) == x


def stats(intervals):
    require(bool(intervals) and all(number(v) and v > 0 for v in intervals), 'intervals must be finite, positive milliseconds')
    ordered = sorted(intervals)
    n = len(ordered)
    total = math.fsum(ordered)
    low = math.ceil(n * .01)
    return dict(sample_count=n, duration_ms=total, average_fps=1000 * n / total,
                low_1_fps=1000 / statistics.mean(ordered[-low:]), mean_ms=total/n,
                p50_ms=ordered[math.ceil(n*.50)-1], p95_ms=ordered[math.ceil(n*.95)-1],
                p99_ms=ordered[math.ceil(n*.99)-1], max_ms=ordered[-1])


def json_object(text):
    def pairs(items):
        out = {}
        for k, v in items:
            require(k not in out, 'duplicate JSON key: ' + k)
            out[k] = v
        return out
    return json.loads(text, object_pairs_hook=pairs,
                      parse_constant=lambda x: (_ for _ in ()).throw(ValueError('nonfinite JSON: '+x)))


def table(text, header):
    reader = csv.DictReader(io.StringIO(text))
    require(tuple(reader.fieldnames or ()) == header, 'incorrect CSV header')
    rows = list(reader)
    require(all(None not in row and all(v is not None for v in row.values()) for row in rows), 'ragged CSV')
    return rows


def validate(files):
    m, s = (json_object(files[name]) for name in ('metadata.json', 'summary.json'))
    for key in ('schema_version', 'run_id', 'preset_id', 'valid', 'invalid_reason'):
        require(key in m and key in s and m[key] == s[key], 'metadata/summary mismatch: '+key)
    require(m['schema_version'] == VERSION, 'unsupported schema')
    require(isinstance(m['run_id'], str) and bool(m['run_id']), 'missing run ID')
    require(m['preset_id'] in PRESETS and type(m['valid']) is bool, 'invalid preset or validity')
    require((m['invalid_reason'] is None) if m['valid'] else isinstance(m['invalid_reason'], str) and bool(m['invalid_reason']), 'invalid reason')
    require(integer(m['repetition'], 1) and m['repetition'] <= 3, 'repetition must be 1..3')
    require(m['warmup_seconds'] == 30 and m['measure_seconds'] == 60, 'wrong protocol duration')
    require(integer(m['started_at'], 1), 'started_at must be Unix milliseconds')
    engine, device, env, scene, settings = (m[k] for k in ('engine', 'device', 'environment', 'scene', 'settings'))
    require(engine['name'] in ENGINES and bool(engine['version']) and engine['build'] in ('debug','release'), 'engine provenance')
    sha = engine['commit']
    require(sha is None or (isinstance(sha,str) and len(sha)==40 and all(c in '0123456789abcdef' for c in sha)), 'invalid commit SHA')
    for k in ('brand', 'model', 'android', 'fingerprint'):
        require(k in device and (device[k] is None or isinstance(device[k],str)), 'missing device '+k)
    require(env['backend'] in ('Vulkan','OpenGL ES','OpenGL','Metal','DX12','unavailable'), 'unknown backend')
    for k in ('surface_size','render_size'):
        require(isinstance(env[k],list) and len(env[k])==2 and all(integer(v,1) for v in env[k]), 'invalid '+k)
    for k in ('dynamic_resolution','gpu_profiling','refresh_limited_possible'):
        require(type(env[k]) is bool, 'invalid '+k)
    for k in ('presentation_mode','gpu','driver'):
        require(env[k] is None or isinstance(env[k],str) and bool(env[k]), 'invalid '+k)
    require(isinstance(env['vsync_requested'],str) and bool(env['vsync_requested']), 'missing VSync policy')
    require(env['refresh_hz'] is None or number(env['refresh_hz'],1), 'invalid refresh rate')
    require(env['thermal_status'] is None or integer(env['thermal_status']) and env['thermal_status']<=6, 'invalid thermal status')
    for k in ('id','seed','camera','animation','geometry_sha256','sources_sha256'):
        require(k in scene and (scene[k] is None or isinstance(scene[k],str)), 'missing scene '+k)
    for k in ('taa','fxaa','shadows','bloom','effects'):
        require(type(settings[k]) is bool, 'invalid setting '+k)
    require(settings['msaa_samples'] in (1,2,4,8) and type(settings['msaa_samples']) is not bool, 'invalid MSAA')
    require(isinstance(settings['pipeline'],str), 'missing pipeline')
    require(isinstance(m['limitations'],list) and all(isinstance(v,str) for v in m['limitations']), 'missing limitations')
    definitions = m['metrics']
    require(isinstance(definitions,dict), 'missing metric definitions')
    for name, d in definitions.items():
        require(d['unit']=='ms' and d['kind'] in ('cpu','gpu','gpu_frame','gpu_pass') and all(isinstance(d[k],str) and d[k] for k in ('source','scope','scope_id')), 'invalid metric definition '+name)
    if m['valid']:
        require(not env['dynamic_resolution'], 'dynamic resolution in valid run')
        require(env['gpu_profiling'] == (m['preset_id']=='gpu'), 'wrong profiling mode')
        scale = .65 if m['preset_id']=='scale_65' else 1
        require(all(abs(a-b*scale)<=1 for a,b in zip(env['render_size'],env['surface_size'])), 'wrong 3D resolution')
        if m['preset_id']=='no_shadows': require(not settings['shadows'], 'shadows still enabled')
        if m['preset_id']=='no_aa': require(settings['msaa_samples']==1 and not settings['taa'] and not settings['fxaa'], 'AA still enabled')
        if m['preset_id']=='no_bloom': require(not settings['bloom'], 'Bloom still enabled')
        if m['preset_id']=='no_effects': require(not settings['effects'], 'effects still enabled')
    frames = table(files['frames.csv'], FRAMES)
    total, intervals = 0., []
    for i,row in enumerate(frames):
        require(row['frame_index']==str(i), 'nonsequential frame index')
        elapsed, dt, animation = (float(row[k]) for k in FRAMES[1:4])
        require(number(dt) and dt>0 and number(elapsed) and number(animation), 'invalid frame timing')
        total += dt
        require(math.isclose(total,elapsed,rel_tol=1e-5,abs_tol=.05), 'frame timeline mismatch')
        intervals.append(dt)
        for k in FRAMES[4:]:
            if row[k]:
                require(k in definitions and number(float(row[k])), 'CPU value without valid scope: '+k)
    expected = stats(intervals) if intervals else None
    require((s['application'] is None)==(expected is None), 'missing frame statistics')
    if expected:
        for k,v in expected.items():
            require(number(s['application'][k]) and math.isclose(s['application'][k],v,rel_tol=1e-5,abs_tol=.01), 'incorrect statistic '+k)
    require(not m['valid'] or total>=60000, 'measurement shorter than 60 seconds')
    require(s['presentation_fps'] is None or number(s['presentation_fps']) and bool(m.get('presentation_source')), 'presentation FPS without source')
    seen, metrics = set(), []
    for row in table(files['metrics.csv'], METRICS):
        key=(row['sample_id'],row['metric'])
        require(key not in seen and row['metric'] in definitions, 'duplicate or undefined metric')
        seen.add(key)
        require(row['sample_id'] and row['observed_frame'].isdigit() and int(row['observed_frame']) < len(frames) and number(float(row['value_ms'])), 'invalid metric observation')
        metrics.append(row)
    return dict(metadata=m, summary=s, frames=frames, metrics=metrics)


def load(paths):
    runs, ids = [], set()
    def add(files):
        run = validate(files)
        run_id = run['metadata']['run_id']
        require(run_id not in ids, 'duplicate run ID: '+run_id)
        ids.add(run_id)
        runs.append(run)
    for path in map(Path,paths):
        if path.is_dir():
            for meta in sorted(path.rglob('metadata.json')):
                if '.pending' in meta.parent.name or (meta.parent/'.pending').exists(): continue
                add({name:(meta.parent/name).read_text() for name in ('metadata.json','summary.json','frames.csv','metrics.csv')})
        else:
            with zipfile.ZipFile(path) as z:
                names = z.namelist()
                require(len(names)==len(set(names)), 'duplicate ZIP member')
                require(sum(i.file_size for i in z.infolist())<=256*1024*1024, 'ZIP exceeds 256 MiB')
                for name in sorted(n for n in names if n.endswith('/metadata.json') or n=='metadata.json'):
                    prefix=name[:-len('metadata.json')]
                    if '.pending' in prefix or prefix+'.pending' in names: continue
                    add({n:z.read(prefix+n).decode('utf-8') for n in ('metadata.json','summary.json','frames.csv','metrics.csv')})
    require(runs, 'no benchmark runs found')
    return runs


def condition(m):
    e, d = m['environment'],m['device']
    hardware=[d[k] for k in ('brand','model','android','fingerprint')]
    scene=[m['scene'][k] for k in ('id','seed','camera','animation','geometry_sha256','sources_sha256')]
    unknown = m['run_id'] if any(v is None or v=='' for v in hardware+scene) or e['backend']=='unavailable' else None
    present=e['presentation_mode'] or 'unavailable:'+m['engine']['name']
    return json.dumps([hardware,scene,m['engine']['build'],e['backend'],e['surface_size'],e['render_size'],e['dynamic_resolution'],e['vsync_requested'],present,e['refresh_hz'],e['gpu_profiling'],m['preset_id'],unknown],sort_keys=True)


def aggregate(values):
    return 'unavailable' if not values else f'{statistics.median(values):.3f} [{min(values):.3f}, {max(values):.3f}]'


def report(runs):
    groups={}
    invalid=[]
    # Do not average different builds/settings of the same engine within a condition.
    signatures={}
    for r in runs:
        m=r['metadata']
        if not m['valid']:
            invalid.append(f"- `{m['run_id']}`: {m['invalid_reason']}")
            continue
        key=condition(m)
        sig=json.dumps([m['engine'],m['settings']],sort_keys=True)
        signatures.setdefault(key,{}).setdefault(m['engine']['name'],set()).add(sig)
        groups.setdefault(key,[]).append(r)
    lines=['# Forest benchmark comparison','', 'Application callback intervals; median [minimum, maximum] of independent runs. All times are milliseconds.', '']
    if any(r['metadata'].get('test_data') for r in runs):
        lines+=['**SYNTHETIC TEST DATA: not device performance; no official ranking.**','']
    csv_rows=[]
    for idx,(key, group) in enumerate(sorted(groups.items()),1):
        by={name:[r for r in group if r['metadata']['engine']['name']==name] for name in ENGINES}
        m=group[0]['metadata']
        lines += [f"## Condition {idx}: {m['preset_id']} / {m['environment']['backend']}", '', f'Conditions: `{key}`', '']
        mixed=any(len(v)>1 for v in signatures[key].values())
        if mixed: lines+=['**Incompatible versions/settings within an engine: aggregation refused.**','']
        if any(len(v)<3 or {r['metadata']['repetition'] for r in v}!={1,2,3} for v in by.values()):
            lines+=['Incomplete three-engine / three-repetition set; no formal ranking.','']
        if m['environment']['refresh_limited_possible']:
            lines+=['Presentation/compositor refresh limits may apply. These results do not establish uncapped renderer throughput.','']
        lines+=['| Metric | Filament | Godot | Bevy |','|---|---|---|---|']
        labels=[('Average FPS','average_fps'),('1% Low FPS','low_1_fps'),('Mean Frame Time','mean_ms'),('Frame Time P50','p50_ms'),('Frame Time P95','p95_ms'),('Frame Time P99','p99_ms'),('Slowest Frame Time','max_ms')]
        for label,field in labels:
            cells=[aggregate([r['summary']['application'][field] for r in by[n]]) if not mixed else 'incompatible' for n in ENGINES]
            lines+=['| '+label+' | '+' | '.join(cells)+' |']
        for label in ('CPU Update Time','CPU Render Time'):
            lines+=['| '+label+' | '+' | '.join('See engine scopes; not directly comparable' if by[n] else 'unavailable' for n in ENGINES)+' |']
        for label,kind in [('GPU Frame Time','gpu_frame'),('GPU Slowest Pass','gpu_pass')]:
            cells=[]
            for name in ENGINES:
                candidates={}
                for r in by[name]:
                    for metric,d in r['metadata']['metrics'].items():
                        if d['kind']!=kind: continue
                        values=[float(x['value_ms']) for x in r['metrics'] if x['metric']==metric]
                        if values: candidates.setdefault((metric,d['scope_id']),[]).append(statistics.mean(values))
                if mixed: cells.append('incompatible')
                elif not candidates: cells.append('unavailable')
                elif kind=='gpu_frame' and len(candidates)>1: cells.append('different scopes; see details')
                else:
                    metric,values=max(candidates.items(),key=lambda x:statistics.median(x[1]))
                    cells.append(f'{aggregate(values)} `{metric[0]}`')
            lines+=['| '+label+' | '+' | '.join(cells)+' |']
        for label,fn in [('3D Resolution',lambda r:'×'.join(str(int(v)) for v in r['metadata']['environment']['render_size'])),('Actual MSAA',lambda r:str(r['metadata']['settings']['msaa_samples'])+'×')]:
            lines+=['| '+label+' | '+' | '.join(', '.join(sorted({fn(r) for r in by[n]})) or 'unavailable' for n in ENGINES)+' |']
        for name in ENGINES:
            if not by[name]: continue
            lines+=['',f'### {name} measured scopes','']
            values={}
            for r in by[name]:
                mm=r['metadata']
                lines += [f"- Run `{mm['run_id']}`, repetition {mm['repetition']}, commit `{mm['engine']['commit']}`.",f"  Actual settings: `{json.dumps(mm['settings'],sort_keys=True)}`"]
                for metric,d in mm['metrics'].items():
                    data=[float(f[metric]) for f in r['frames'] if metric in f and f[metric]]
                    data += [float(x['value_ms']) for x in r['metrics'] if x['metric']==metric]
                    if data: values.setdefault((metric,d['source'],d['scope'],d['scope_id']),[]).append(statistics.mean(data))
                csv_rows.append(dict(condition=idx,run_id=mm['run_id'],engine=name,preset_id=mm['preset_id'],backend=mm['environment']['backend'],**r['summary']['application']))
            for (metric,source,scope,_),v in sorted(values.items()):
                lines+= [f'- `{metric}`: {aggregate(v)} ms. Source: {source}. Scope: {scope}.']
            lines+=['- Limitation: '+v for v in sorted({v for r in by[name] for v in r['metadata']['limitations']})]
        lines+=['']
    if invalid: lines+=['## Excluded invalid runs','']+invalid
    return '\n'.join(lines)+'\n', csv_rows


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('inputs',nargs='+')
    parser.add_argument('--validate-only',action='store_true')
    parser.add_argument('--output',default='comparison')
    args=parser.parse_args()
    try:
        runs=load(args.inputs)
        if not args.validate_only:
            md,rows=report(runs)
            Path(args.output+'.md').write_text(md)
            fields=['condition','run_id','engine','preset_id','backend','sample_count','duration_ms','average_fps','low_1_fps','mean_ms','p50_ms','p95_ms','p99_ms','max_ms']
            with open(args.output+'.csv','w',newline='') as f:
                writer=csv.DictWriter(f,fieldnames=fields);writer.writeheader();writer.writerows(rows)
        print(f'Validated {len(runs)} runs')
    except (OSError,ValueError,KeyError,TypeError,zipfile.BadZipFile) as e:
        parser.exit(1,f'Benchmark rejected: {e}\n')


if __name__=='__main__': main()
