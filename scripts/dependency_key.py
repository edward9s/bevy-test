#!/usr/bin/env python3
"""Exact dependency ABI/profile key; application source/assets never affect this key."""
import hashlib
import pathlib
import subprocess

root = pathlib.Path(__file__).resolve().parents[1]
hash = hashlib.sha256(b'bevy-deps-v1:ubuntu-24.04:x86_64:aarch64-linux-android:release\0')
for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml',
             'scripts/android-env.sh', 'scripts/dependency_key.py'):
    hash.update(name.encode() + b'\0' + (root/name).read_bytes() + b'\0')
hash.update(subprocess.check_output(['rustc', '-vV']))
print('bevy-deps-' + hash.hexdigest()[:32])
