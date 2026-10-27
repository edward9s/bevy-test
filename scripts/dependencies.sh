#!/usr/bin/env bash
set -euo pipefail
mode="${1:?expected pack or restore}"
archive="${2:?expected archive directory}"
case "$mode" in
  pack)
    mkdir -p "$archive" "$HOME/.cargo/git"
    # The scaffold is never restored as a finished application.
    find target -type d -name 'bevy-forest-*' -prune -exec rm -rf {} +
    find target -type f -name '*bevy_forest*' -delete
    tar -I 'zstd -T2 -5' -cf "$archive/deps.tar.zst" target -C "$HOME" .cargo/registry .cargo/git
    # GitHub Release limits individual assets to 2 GiB.
    split -b 1800M "$archive/deps.tar.zst" "$archive/deps.tar.zst.part-"
    rm "$archive/deps.tar.zst"
    (cd "$archive" && sha256sum deps.tar.zst.part-* > deps.sha256)
    ;;
  restore)
    (cd "$archive" && sha256sum -c deps.sha256)
    # Split the two roots so no archive needs to extract absolute paths.
    cat "$archive"/deps.tar.zst.part-* | tar -I zstd -xf - target
    cat "$archive"/deps.tar.zst.part-* | tar -I zstd -xf - -C "$HOME" .cargo
    ;;
  *) echo "Unknown dependency operation: $mode" >&2; exit 1 ;;
esac
