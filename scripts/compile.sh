#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
mkdir -p build
RUSTC=${RUSTC:-rustc}
if [ -n "${AVALANCHE_SYSROOT:-}" ]; then
    set -- --sysroot "$AVALANCHE_SYSROOT"
else
    set --
fi
"$RUSTC" "$@" --edition=2021 --target x86_64-unknown-none -C panic=abort -C opt-level=2 -C relocation-model=static -C code-model=kernel -C link-arg=-Tkernel/linker.ld -C link-arg=--build-id=none -D warnings kernel/main.rs -o build/avalanche.elf
