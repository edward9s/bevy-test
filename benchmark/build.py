#!/usr/bin/env python3
"""Bake source provenance; generated files are deliberately not committed."""
import hashlib
import json
from pathlib import Path
import subprocess

root=Path(__file__).resolve().parents[1]
def digest(data): return hashlib.sha256(data).hexdigest()
if (root/'filament.json').exists():
    name='Filament'; version=json.loads((root/'filament.json').read_text())['version']; geometry=root/'src/Geometry.cpp'
else:
    name='Bevy' if (root/'Cargo.toml').exists() else 'Godot'
    version='0.20.0' if name=='Bevy' else '4.7.2'
    geometry=root/'vendor/Geometry.cpp'
text=geometry.read_text().strip()
# Filament's bit-identical indexing changes storage, not the generator. Record
# both the original source hash and this explicitly canonicalized identity.
original=digest(text.encode())
if name=='Filament':
    for include in ('#include <bit>\n','#include <stdexcept>\n','#include <unordered_map>\n'):
        text=text.replace(include,'')
    start=text.index('void compact(Mesh& mesh) {'); end=text.index('float elevation(',start)
    text=text[:start]+text[end:]
    text=text.replace('for(auto& [key,m]:b.batches) {compact(m);result.push_back(std::move(m));}', 'for(auto& [key,m]:b.batches) result.push_back(std::move(m));')
sources=json.loads((root/'assets/sources.json').read_text())
engine=dict(name=name,version=version,commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),build='release',geometry_source_sha256=original)
scene=dict(id='forest-v1',seed='compiled-deterministic',camera='orbit-yaw0.24-pitch0.12-distance13-target0,1.6,-3-fov52',animation='monotonic-seconds-since-warmup; start=0',geometry_sha256=digest(text.encode()),sources_sha256=digest(json.dumps(sources,sort_keys=True,separators=(',',':')).encode()),geometry_identity='canonical generator source; not a binary mesh hash')
engine['shaders']={str(p.relative_to(root)):digest(p.read_bytes()) for folder in ('shaders','materials','assets/shaders') for p in sorted((root/folder).rglob('*')) if p.is_file()}
data=dict(engine=engine,scene=scene)
(root/'benchmark/build.json').write_text(json.dumps(data,indent=2)+'\n')
if name=='Filament':
    (root/'benchmark/Build.h').write_text('#pragma once\nnamespace benchmark {\ninline constexpr auto engineJson=R"json('+json.dumps(engine)+')json";\ninline constexpr auto sceneJson=R"json('+json.dumps(scene)+')json";\n}\n')
elif name=='Bevy':
    (root/'src/benchmark_build.rs').write_text('#[rustfmt::skip]\npub const ENGINE: &str = r#"'+json.dumps(engine)+'"#;\n#[rustfmt::skip]\npub const SCENE: &str = r#"'+json.dumps(scene)+'"#;\n')
print(name,engine['commit'],scene['geometry_sha256'],scene['sources_sha256'])
