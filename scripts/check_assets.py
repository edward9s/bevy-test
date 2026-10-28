#!/usr/bin/env python3
"""Validate generated topology and texture headers before packaging."""
import json
import pathlib
import struct
import numpy as np

root = pathlib.Path(__file__).resolve().parents[1] / 'assets/generated'
data = (root/'forest.glb').read_bytes()
magic, version, size = struct.unpack_from('<4sII', data)
assert (magic,version,size) == (b'glTF',2,len(data))
length, kind = struct.unpack_from('<I4s',data,12)
assert kind == b'JSON'
model = json.loads(data[20:20+length])
binary_start = 28+length
assert len(model['meshes']) == 232
assert not model['images']
triangles = 0
for mesh in model['meshes']:
    for primitive in mesh['primitives']:
        positions = model['accessors'][primitive['attributes']['POSITION']]
        indices = model['accessors'][primitive['indices']]
        view = model['bufferViews'][indices['bufferView']]
        assert indices['componentType'] in (5123,5125)
        values = np.frombuffer(data, '<u2' if indices['componentType']==5123 else '<u4', indices['count'], binary_start+view['byteOffset'])
        assert values.max() < positions['count'] and len(values)%3 == 0
        triangles += len(values)//3
        for name,width in [('POSITION',3),('NORMAL',3),('TANGENT',4),('TEXCOORD_0',2),('COLOR_0',4)]:
            attr=model['accessors'][primitive['attributes'][name]]
            view=model['bufferViews'][attr['bufferView']]
            assert attr['componentType']==5126 and attr['count']==positions['count']
            assert view['byteLength']==attr['count']*width*4
            vertices=np.frombuffer(data,'<f4',attr['count']*width,binary_start+view['byteOffset'])
            assert np.isfinite(vertices).all()
assert triangles == 385622
for name in ['ground_albedo','ground_normal','ground_arm','stone_albedo','stone_normal',
             'stone_arm','bark_albedo','bark_normal','bark_arm','canopy','sky','diffuse','specular']:
    content = (root/f'{name}.dds').read_bytes()
    assert content[:4] == b'DDS '
    header = struct.unpack_from('<31I',content,4)
    dxgi, dimension, cube, layers, _ = struct.unpack_from('<5I',content,128)
    assert header[0] == 124 and header[18:21] == (32,4,int.from_bytes(b'DX10','little'))
    assert dimension == 3 and layers == 1 and dxgi in (10,28)
    width,height,levels = header[3],header[2],header[6]
    if name not in ('sky','diffuse','specular'):
        assert (width,height,levels)==(1024,1024,11), name
    expected = sum(max(1,width>>i)*max(1,height>>i) for i in range(levels))*(8 if dxgi==10 else 4)*(6 if cube else 1)
    assert len(content) == 148+expected, name
    assert bool(cube) == (name in ('sky','diffuse','specular'))
    if dxgi == 10:
        assert np.isfinite(np.frombuffer(content,'<f2',offset=148)).all()
for name in ['ground_albedo','ground_normal','ground_arm','stone_albedo','stone_normal',
             'stone_arm','bark_albedo','bark_normal','bark_arm','canopy']:
    content = (root/f'{name}.ktx2').read_bytes()
    assert content[:12] == b'\xabKTX 20\xbb\r\n\x1a\n'
    fmt, _, width, height, depth, layers, faces, levels, scheme = struct.unpack_from('<9I',content,12)
    assert fmt == (158 if name.endswith('albedo') or name=='canopy' else 157), name
    assert (width,height,depth,layers,faces,levels,scheme) == (1024,1024,0,0,1,11,2), name
    for level in range(levels):
        offset, size, unpacked = struct.unpack_from('<3Q',content,80+24*level)
        assert 0 < size and offset+size <= len(content)
        expected = max(1,(max(1,width>>level)+3)//4)*max(1,(max(1,height>>level)+3)//4)*16
        assert unpacked == expected, (name,level)
print(f'Assets passed: 232 batches, {triangles} triangles, 13 DDS + 10 native ASTC textures')
