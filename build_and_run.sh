#!/usr/bin/env bash
set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")"

pkill -x easyflp-gui 2>/dev/null || true
pkill -x easyflp 2>/dev/null || true

cargo build --release

nohup ./target/release/easyflp >/dev/null 2>&1 &
