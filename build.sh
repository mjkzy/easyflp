#!/usr/bin/env bash
set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")"

cargo build --release
