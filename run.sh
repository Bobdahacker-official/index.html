#!/bin/sh
set -eu
cd "$(dirname "$0")"
RAM=${1:-32G}
case "$RAM" in 4G|8G|16G|32G) ;; *) echo 'Usage: run.sh [4G|8G|16G|32G]' >&2; exit 2;; esac
OVMF=${OVMF:-/usr/share/OVMF/OVMF_CODE_4M.fd}
OVMF_VARS=${OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
AVALANCHE_DISK=${AVALANCHE_DISK:-build/runtime.img}
if [ ! -e "$AVALANCHE_DISK" ]; then
    cp build/avalancheos.img "$AVALANCHE_DISK"
fi
python3 - "$OVMF_VARS" "$OVMF" <<'PY'
import pathlib,sys
pathlib.Path('build/firmware.fd').write_bytes(pathlib.Path(sys.argv[1]).read_bytes()+pathlib.Path(sys.argv[2]).read_bytes())
PY
ACCEL=tcg
if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then ACCEL=kvm; fi
exec "${QEMU:-qemu-system-x86_64}" -machine q35 -accel "$ACCEL" -smp 4 -m "$RAM" -bios build/firmware.fd -boot order=d -cdrom build/avalancheos.iso -drive "file=$AVALANCHE_DISK,format=raw,if=none,id=avadisk" -device virtio-blk-pci,drive=avadisk,disable-modern=on -serial stdio -nic none -no-reboot
