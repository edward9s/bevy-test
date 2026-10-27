"""Offline HDR cubemap filtering and portable, uncompressed DDS mip chains.

No runtime convolution or texture mip generation; RGBA16F preserves HDR radiance.
"""
import math
import struct

import numpy as np
from PIL import Image


def dds(path, faces, floating=False):
    levels = len(faces[0])
    height, width = faces[0][0].shape[:2]
    cube = len(faces) == 6
    assert len(faces) in (1, 6) and (not cube or height == width)
    header = [124, 0x2100F, height, width, width * (8 if floating else 4), 0, levels]
    header += [0] * 11
    header += [32, 4, int.from_bytes(b'DX10', 'little'), 0, 0, 0, 0, 0]
    header += [0x401008, 0xFE00 if cube else 0, 0, 0, 0]
    dx10 = [10 if floating else 28, 3, 4 if cube else 0, 1, 0]
    with path.open('wb') as output:
        output.write(b'DDS ' + struct.pack('<31I', *header) + struct.pack('<5I', *dx10))
        for face in faces:
            for level in face:
                assert level.shape[2] == 4 and np.isfinite(level).all()
                output.write(level.astype('<f2' if floating else 'u1').tobytes())


def texture(path, output, srgb=False, normal=False):
    value = np.asarray(Image.open(path).convert('RGBA'), dtype=np.float32) / 255
    if srgb:
        value[..., :3] = np.where(value[..., :3] <= .04045,
                                 value[..., :3] / 12.92,
                                 ((value[..., :3] + .055) / 1.055) ** 2.4)
    assert value.shape == (1024,1024,4)
    value = value.reshape(512,2,512,2,4).mean(axis=(1,3))
    chain = []
    while True:
        encoded = value.copy()
        if normal:
            n = encoded[..., :3] * 2 - 1
            n /= np.maximum(np.linalg.norm(n, axis=-1, keepdims=True), 1e-6)
            encoded[..., :3] = n * .5 + .5
        if srgb:
            encoded[..., :3] = np.where(encoded[..., :3] <= .0031308,
                encoded[..., :3] * 12.92, 1.055 * encoded[..., :3] ** (1/2.4) - .055)
        chain.append(np.rint(np.clip(encoded, 0, 1) * 255).astype('u1'))
        h, w = value.shape[:2]
        if h == w == 1:
            break
        assert h % 2 == w % 2 == 0
        value = value.reshape(h//2, 2, w//2, 2, 4).mean(axis=(1, 3))
    dds(output, [chain])


def radiance(path):
    with path.open('rb') as source:
        assert source.readline().startswith(b'#?')
        while source.readline().strip():
            pass
        orientation = source.readline().split()
        assert orientation[0] == b'-Y' and orientation[2] == b'+X'
        h, w = int(orientation[1]), int(orientation[3])
        data = np.empty((h, w, 4), dtype='u1')
        for y in range(h):
            assert source.read(4) == bytes([2, 2, w >> 8, w & 255])
            for c in range(4):
                x = 0
                while x < w:
                    code = source.read(1)[0]
                    assert code > 0
                    count = code - 128 if code > 128 else code
                    assert x + count <= w
                    values = source.read(1 if code > 128 else count)
                    data[y, x:x+count, c] = values[0] if code > 128 else np.frombuffer(values, dtype='u1')
                    x += count
    return data[..., :3].astype('f4') * np.exp2(data[..., 3:4].astype('f4') - 136)


def directions(size, face):
    uv = (np.arange(size, dtype='f4') + .5) / size * 2 - 1
    u, v = np.meshgrid(uv, uv)
    one = np.ones_like(u)
    # Vulkan/DDS cube face order: +X, -X, +Y, -Y, +Z, -Z.
    vectors = [(one, -v, -u), (-one, -v, u), (u, one, v),
               (u, -one, -v), (u, -v, one), (-u, -v, -one)]
    n = np.stack(vectors[face], axis=-1).reshape(-1, 3)
    return n / np.linalg.norm(n, axis=-1, keepdims=True)


def sample(hdr, vectors):
    h, w = hdr.shape[:2]
    u = (np.arctan2(vectors[..., 0], vectors[..., 2]) / (2*math.pi) + .5) * w - .5
    v = np.arccos(np.clip(vectors[..., 1], -1, 1)) / math.pi * h - .5
    x, y = np.floor(u).astype('i4'), np.floor(v).astype('i4')
    a, b = (u-x)[..., None], (v-y)[..., None]
    top = hdr[np.clip(y, 0, h-1), x % w] * (1-a) + hdr[np.clip(y, 0, h-1), (x+1) % w] * a
    bottom = hdr[np.clip(y+1, 0, h-1), x % w] * (1-a) + hdr[np.clip(y+1, 0, h-1), (x+1) % w] * a
    return top * (1-b) + bottom * b


def filter_cube(hdr, size, roughness=None, diffuse=False):
    faces = []
    samples = 128
    # Hammersley points avoid random changes between builds.
    bits = np.arange(samples, dtype='u4')
    radical = np.zeros(samples)
    for bit in range(32):
        radical += ((bits >> bit) & 1) * 2.0 ** (-bit - 1)
    phi = (np.arange(samples) + .5) / samples * 2 * math.pi
    if diffuse:
        cosine = np.sqrt(1-radical)
    else:
        a = max(roughness or 0, .001) ** 2
        cosine = np.sqrt((1-radical) / (1+(a*a-1)*radical))
    sine = np.sqrt(1-cosine*cosine)
    local = np.stack((np.cos(phi)*sine, np.sin(phi)*sine, cosine), axis=-1)
    for face in range(6):
        normal = directions(size, face)
        result = np.empty((size*size, 4), dtype='f4')
        for start in range(0, len(normal), 1024):
            n = normal[start:start+1024]
            if roughness == 0 and not diffuse:
                rgb = sample(hdr, n)
            else:
                axis = np.tile([0., 0., 1.], (len(n), 1))
                axis[np.abs(n[:, 2]) > .99] = [1., 0., 0.]
                tangent = np.cross(axis, n)
                tangent /= np.linalg.norm(tangent, axis=-1, keepdims=True)
                bitangent = np.cross(n, tangent)
                half = (tangent[:, None] * local[None, :, 0:1] +
                        bitangent[:, None] * local[None, :, 1:2] + n[:, None] * local[None, :, 2:3])
                light = half if diffuse else 2*np.sum(n[:, None]*half, axis=-1, keepdims=True)*half-n[:, None]
                weight = np.ones(light.shape[:2]) if diffuse else np.maximum(np.sum(n[:, None]*light, axis=-1), 0)
                rgb = np.sum(sample(hdr, light)*weight[..., None], axis=1) / np.maximum(weight.sum(axis=1, keepdims=True), 1e-6)
            result[start:start+len(n), :3] = rgb
            result[start:start+len(n), 3] = 1
        faces.append(result.reshape(size, size, 4))
    return faces


def environment(source, dest):
    hdr = radiance(source)
    sky = filter_cube(hdr, 256, 0)
    dds(dest/'sky.dds', [[face] for face in sky], True)
    diffuse = filter_cube(hdr, 32, diffuse=True)
    dds(dest/'diffuse.dds', [[face] for face in diffuse], True)
    specular = [[] for _ in range(6)]
    for mip in range(9):
        for face, pixels in enumerate(filter_cube(hdr, max(1, 256 >> mip), mip/8)):
            specular[face].append(pixels)
    dds(dest/'specular.dds', specular, True)
