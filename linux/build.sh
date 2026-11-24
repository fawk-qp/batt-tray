#!/usr/bin/env bash
# Builds the Linux C++ and Rust binaries.
# Requires: g++, pkg-config, libdbus-1-dev, cargo (Rust only).
set -euo pipefail
cd "$(dirname "$0")"

OUT="${1:-../dist}"
mkdir -p "$OUT"
DBUS_FLAGS="$(pkg-config --cflags --libs dbus-1)"

echo "==> linux/cpp"
g++ -std=c++17 -O2 -s -Wall -Wextra \
    cpp/main.cpp cpp/device.cpp cpp/scanner.cpp cpp/sni.cpp \
    -o "$OUT/batt-tray-linux-cpp" $DBUS_FLAGS

echo "==> linux/rust"
(cd rust && cargo build --release)
cp rust/target/release/batt-tray "$OUT/batt-tray-linux-rust"

echo "Done:"
ls -la "$OUT"/batt-tray-linux-*
