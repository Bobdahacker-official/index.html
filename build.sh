#!/bin/sh
set -eu
cd "$(dirname "$0")"
sh scripts/compile.sh
python3 scripts/images.py
