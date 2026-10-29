#!/usr/bin/env python3
"""Check the benchmark runtime, ABI, DEX and build provenance in a packaged APK."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import zipfile
import zlib


def check(path,engine,commit):
    def require(ok,message):
        if not ok: raise ValueError(message)
    with zipfile.ZipFile(path) as apk:
        names=apk.namelist()
        require(len(names)==len(set(names)), 'duplicate APK entries')
        require('AndroidManifest.xml' in names, 'missing Android manifest')
        dex=apk.read('classes.dex')
        require(dex.startswith(b'dex\n') and len(dex)>=112, 'invalid DEX header')
        require(struct.unpack_from('<I',dex,8)[0]==zlib.adler32(dex[12:]) & 0xffffffff, 'DEX checksum mismatch')
        require(dex[12:32]==hashlib.sha1(dex[32:]).digest(), 'DEX signature mismatch')
        libraries=[n for n in names if n.startswith('lib/') and n.endswith('.so')]
        require(libraries and all(n.startswith('lib/arm64-v8a/') for n in libraries), 'APK must contain arm64 libraries only')
        native={'Filament':'libmain.so','Godot':'libgodot_android.so','Bevy':'libbevy_forest.so'}[engine]
        name='lib/arm64-v8a/'+native
        require(apk.getinfo(name).file_size<256*1024*1024, 'unexpected native library size')
        data=apk.read(name)
        require(data[:4]==b'\x7fELF' and data[4]==2 and data[18:20]==b'\xb7\x00', 'native library is not AArch64 ELF64')
        if engine=='Godot':
            require('assets/benchmark/controller.gdc' in names and 'assets/benchmark/protocol.gdc' in names, 'missing compiled benchmark scripts')
            provenance=json.loads(apk.read('assets/benchmark/build.json'))
            require(provenance['engine']['name']=='Godot' and provenance['engine']['commit']==commit, 'incorrect Godot provenance')
            require(len(provenance['engine']['source_sha256'])==64, 'missing source hash')
        else:
            require(b'Lbenchmark/Bridge;' in dex, 'missing Android benchmark bridge')
            if engine=='Bevy': require(b'Lbenchmark/BenchmarkActivity;' in dex, 'missing NativeActivity subclass')
            for method in ('command','status','repetition'):
                require(('Java_benchmark_Bridge_'+method).encode() in data, 'missing benchmark JNI export: '+method)
            require(commit.encode() in data, 'native library lacks expected build SHA')
            asset='assets/forest/effects.filamat' if engine=='Filament' else 'assets/generated/forest.glb'
            require(asset in names, 'missing forest asset')
    return f'{engine}: benchmark APK structure, arm64 ABI, DEX and commit verified'


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('engine',choices=('Filament','Godot','Bevy'))
    p.add_argument('apk',type=Path)
    p.add_argument('--commit',required=True)
    args=p.parse_args()
    try: print(check(args.apk,args.engine,args.commit))
    except (OSError,ValueError,KeyError,zipfile.BadZipFile) as error:p.exit(1,f'APK rejected: {error}\n')


if __name__=='__main__':main()
