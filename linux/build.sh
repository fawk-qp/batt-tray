#!/usr/bin/env bash
# Builds the Linux C++ and Rust binaries.
# Requires: g++, pkg-config, libdbus-1-dev, cargo (Rust only).
set -euo pipefail
cd "$(dirname "$0")"

OUT="${1:-../dist}"
mkdir -p "$OUT"
DBUS_FLAGS="$(pkg-config --cflags --libs dbus-1)"

echo "==> linux/cpp"
g++ -std=c++17 -Os -fno-exceptions -fno-rtti -flto -ffunction-sections -fdata-sections \
    -fPIE -fstack-protector-strong -D_FORTIFY_SOURCE=2 -Wall -Wextra -Werror \
    cpp/*.cpp -o "$OUT/batt-tray-linux-cpp" $DBUS_FLAGS \
    -pie -Wl,--gc-sections,-z,relro,-z,now -s

echo "==> linux/rust"
(cd rust && \
    RUSTFLAGS="-C relocation-model=pie -C link-arg=-Wl,-z,relro,-z,now" \
    cargo build --release)
cp rust/target/release/batt-tray-linux "$OUT/batt-tray-linux-rust"

echo "Done:"
ls -la "$OUT"/batt-tray-linux-*
