#!/usr/bin/env python3
"""Export the reference geometry, mipmapped PBR textures and HDR IBL for Bevy."""
import hashlib
import io
import json
import pathlib
import struct
import subprocess
import tempfile

from PIL import Image
import numpy as np
from environment import environment, texture

ROOT = pathlib.Path(__file__).resolve().parents[1]
HOST_URL = "https://github.com/google/filament/releases/download/v1.78.1/filament-v1.78.1-linux.tgz"
HOST_SHA256 = "9e88f9d6aee7956c081448f237305988570a5bca76fed6e0d88ee1137110af1f"
CACHE = ROOT / ".deps"
DEST = ROOT / "assets/generated"
SURFACES = ("Ground", "Stone", "Bark", "Leaves", "Canopy")


def download(url, target, expected):
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_file() and hashlib.sha256(target.read_bytes()).hexdigest() == expected:
        return
    with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as tmp:
        temporary = pathlib.Path(tmp.name)
    try:
        subprocess.run(["curl", "-fLsS", "--retry", "3", "--connect-timeout", "20",
                        "--max-time", "300", url, "-o", str(temporary)], check=True)
        if hashlib.sha256(temporary.read_bytes()).hexdigest() != expected:
            raise ValueError(f"SHA-256 mismatch: {url}")
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)


def pad(data, fill=b"\0"):
    return data + fill * (-len(data) % 4)


def orm_bytes(path):
    # The scene's metallicFactor is zero. ORMMaterial3D reads B directly, so
    # bake that factor while retaining the exact decoded AO/roughness channels.
    with Image.open(path) as picture:
        red, green, _ = picture.convert("RGB").split()
        packed = Image.merge("RGB", (red, green, Image.new("L", red.size, 0)))
        output = io.BytesIO()
        packed.save(output, format="PNG")
        return output.getvalue()


def build_glb(binary_geometry, output):
    payload = bytearray()
    model = {
        "asset": {"version": "2.0", "generator": "godot-test reference geometry -> Bevy"},
        "scene": 0, "scenes": [{"nodes": []}], "nodes": [],
        "meshes": [], "accessors": [], "bufferViews": [], "buffers": [{"byteLength": 0}],
        "materials": [], "images": [], "samplers": [{
            "magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497
        }], "textures": []
    }

    def append_view(content, stride=None, target=None):
        offset = len(payload)
        payload.extend(pad(content))
        view = {"buffer": 0, "byteOffset": offset, "byteLength": len(content)}
        if stride is not None:
            view["byteStride"] = stride
        if target is not None:
            view["target"] = target
        index = len(model["bufferViews"])
        model["bufferViews"].append(view)
        return index

    def accessor(view, offset, count, shape, kind=5126, limits=None):
        result = {"bufferView": view, "byteOffset": offset, "componentType": kind,
                  "count": count, "type": shape}
        if limits is not None:
            result["min"], result["max"] = limits
        index = len(model["accessors"])
        model["accessors"].append(result)
        return index

    for name in ("ground", "stone", "bark"):
        # The Bevy port uses external DDS materials with offline mipmaps.
        pbr = {"metallicFactor": 0.0, "roughnessFactor": 1.0}
        model["materials"].append({
            "name": name.capitalize(), "pbrMetallicRoughness": pbr,
        })
    model["materials"].append({
        "name": "Leaves", "doubleSided": True,
        "pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 0.68}
    })
    model["materials"].append({
        "name": "Canopy", "doubleSided": True, "alphaMode": "MASK",
        "alphaCutoff": 0.4, "pbrMetallicRoughness": {
            "metallicFactor": 0.0, "roughnessFactor": 0.8
        }
    })
    with binary_geometry.open("rb") as source:
        def read(size):
            result = source.read(size)
            if len(result) != size:
                raise ValueError("Truncated geometry stream")
            return result

        if read(4) != b"GEO1":
            raise ValueError("Wrong geometry stream signature")
        batch_count = struct.unpack("<I", read(4))[0]
        if not 100 <= batch_count <= 500:
            raise ValueError(f"Unexpected batch count: {batch_count}")
        triangles = 0
        for batch in range(batch_count):
            surface, count, index_count = struct.unpack("<III", read(12))
            minimum, maximum = struct.unpack("<3f", read(12)), struct.unpack("<3f", read(12))
            if surface >= len(SURFACES) or count == 0 or index_count % 3:
                raise ValueError("Invalid generated mesh metadata")
            vertices = np.frombuffer(read(count * 64), dtype='<f4').reshape(-1,16).copy()
            # Quantize only the shading frame, not positions, UVs or wind weights.
            # Tiny floating-point differences otherwise prevent shared vertices.
            vertices[:,3:10] = np.round(vertices[:,3:10],4)
            vertices[vertices == 0] = 0  # Canonicalize negative zero before byte deduplication.
            vertices = vertices.view('V64').reshape(-1)
            original_indices = np.frombuffer(read(index_count * 4), dtype='<u4')
            unique, first, inverse = np.unique(vertices, return_index=True, return_inverse=True)
            # Keep the first-use order so adjacent triangles fetch nearby vertices.
            order = np.argsort(first)
            remap = np.empty_like(order)
            remap[order] = np.arange(len(order))
            unique, inverse = unique[order], remap[inverse]
            count = len(unique)
            values = unique.view('<f4').reshape(-1,16)
            index_type = '<u2' if count <= 65536 else '<u4'
            index_view = append_view(inverse[original_indices].astype(index_type).tobytes(), target=34963)
            def attribute(start,end,shape,limits=None):
                view=append_view(values[:,start:end].tobytes(),target=34962)
                return accessor(view,0,count,shape,limits=limits)
            # Separate streams compress repeated UVs/colors much better in the APK.
            positions = attribute(0,3,"VEC3",(minimum,maximum))
            normals = attribute(3,6,"VEC3")
            tangents = attribute(6,10,"VEC4")
            uv = attribute(10,12,"VEC2")
            colors = attribute(12,16,"VEC4")
            indices = accessor(index_view, 0, index_count, "SCALAR", kind=5123 if index_type=='<u2' else 5125)
            name = f"{SURFACES[surface]}_{batch:03d}"
            mesh_id = len(model["meshes"])
            model["meshes"].append({
                "name": name, "primitives": [{
                    "attributes": {"POSITION": positions, "NORMAL": normals,
                                   "TANGENT": tangents, "TEXCOORD_0": uv, "COLOR_0": colors},
                    "indices": indices, "material": surface, "mode": 4
                }]
            })
            model["nodes"].append({"name": name, "mesh": mesh_id})
            model["scenes"][0]["nodes"].append(mesh_id)
            triangles += index_count // 3
        if source.read(1):
            raise ValueError("Trailing geometry bytes")
    if not 300000 <= triangles <= 450000:
        raise ValueError(f"Unexpected scene triangle count: {triangles}")
    model["buffers"][0]["byteLength"] = len(payload)
    document = pad(json.dumps(model, separators=(",", ":"), allow_nan=False).encode(), b" ")
    body = pad(bytes(payload))
    header = struct.pack("<4sII", b"glTF", 2, 12 + 8 + len(document) + 8 + len(body))
    output.write_bytes(header + struct.pack("<I4s", len(document), b"JSON") + document
                       + struct.pack("<I4s", len(body), b"BIN\0") + body)
    print(f"GLB: {batch_count} batches, {triangles} triangles, {output.stat().st_size:,} bytes")


def main():
    manifest = json.loads((ROOT / "assets/sources.json").read_text())
    sources = {}
    for entry in manifest:
        ext = pathlib.PurePosixPath(entry["url"]).suffix
        target = CACHE / "sources" / (entry["name"] + ext)
        download(entry["url"], target, entry["sha256"])
        sources[entry["name"]] = target
        if entry["name"] != "environment":
            with Image.open(target) as photo:
                if photo.size != (1024, 1024):
                    raise ValueError(f"Unexpected source dimensions: {entry['name']}")

    sdk_archive = CACHE / "filament-host-1.78.1.tgz"
    download(HOST_URL, sdk_archive, HOST_SHA256)
    sdk = CACHE / "host"
    headers = sdk / "filament/include"
    if not (headers / "math/vec3.h").is_file():
        sdk.mkdir(parents=True, exist_ok=True)
        subprocess.run(["tar", "--no-same-owner", "-xzf", str(sdk_archive), "-C", str(sdk)], check=True)
    if not (headers / "math/vec3.h").is_file():
        raise RuntimeError("Filament host SDK headers missing")

    DEST.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=CACHE) as tmp:
        temp = pathlib.Path(tmp)
        tool = temp / "export_geometry"
        subprocess.run(["g++", "-std=c++20", "-O2", "-Wall", "-Wextra",
                        "-I", str(headers), str(ROOT / "vendor/Geometry.cpp"),
                        str(ROOT / "scripts/export_geometry.cpp"), "-o", str(tool)], check=True)
        geometry = temp / "geometry.bin"
        subprocess.run([str(tool), str(geometry)], check=True)

        canopy = temp / "canopy.png"
        with Image.open(sources["canopy"]) as picture:
            crop = next(item["crop"] for item in manifest if item["name"] == "canopy")
            picture.convert("RGBA").crop(crop).resize((1024, 1024), Image.Resampling.LANCZOS).save(canopy)
        output = DEST / "forest.glb"
        build_glb(geometry, output)
        for name in ('ground', 'stone', 'bark'):
            for channel in ('albedo', 'normal', 'arm'):
                key = f'{name}_{channel}'
                # Match the zero metallic factor in the source glTF.
                image = sources[key]
                if channel == 'arm':
                    image = temp / f'{key}.png'
                    image.write_bytes(orm_bytes(sources[key]))
                texture(image, DEST / f'{key}.dds', srgb=channel == 'albedo', normal=channel == 'normal')
        texture(canopy, DEST / 'canopy.dds', srgb=True)
        # Native ASTC KTX2 uses the pinned offline encoder, with no runtime transcoder.
        encoder = sdk / 'filament/bin/basisu'
        for name in ('ground', 'stone', 'bark', 'canopy'):
            channels = ('albedo', 'normal', 'arm') if name != 'canopy' else ('',)
            for channel in channels:
                key = f'{name}_{channel}' if channel else name
                space = '-srgb' if channel == 'albedo' or name == 'canopy' else '-linear'
                subprocess.run([str(encoder), '-file', str(DEST / f'{key}.dds'),
                    '-astc_ldr_4x4', '-effort', '3', '-max_threads', '2', space,
                    '-output_file', str(DEST / f'{key}.ktx2')], check=True)
        environment(sources['environment'], DEST)
    print("Bevy scene assets ready (offline mipmaps and HDR IBL).")


if __name__ == "__main__":
    main()
