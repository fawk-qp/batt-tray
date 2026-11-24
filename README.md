# batt-tray

Native system-tray battery indicators for wireless peripherals (mouse, keyboard, headphones). Shows one tray icon per device kind with battery level, charging state and low-battery overlay.

- Windows: native tray icons rendered from `Segoe Fluent Icons` / `Segoe MDL2 Assets`, BLE GATT + PnP battery sources.
- Linux: StatusNotifierItem (SNI) + dbusmenu, no GUI toolkits — works on KDE Plasma, GNOME (AppIndicator), Waybar/Hyprland/Sway, XFCE.

Left click does nothing; right-click opens a menu (`Refresh now`, `Run at startup` on Windows / `Exit` on Linux). All details are in the icon tooltip.

## Platforms and binaries

| Platform | Language | Output |
|---|---|---|
| Windows | C++17 (MSVC) | `batt-tray-win-cpp.exe` |
| Windows | Rust (0 crates) | `batt-tray-win-rust.exe` |
| Linux   | C++17 (libdbus-1) | `batt-tray-linux-cpp` |
| Linux   | Rust (0 crates, raw libdbus FFI) | `batt-tray-linux-rust` |

## Build

### Windows
```
build.bat
```
Requires MSVC (`cl`) and/or Rust (`cargo`). Binaries land in `dist\`.

### Linux
```
sudo apt install g++ pkg-config libdbus-1-dev   # + cargo for Rust
./linux/build.sh [output-dir]
```

## Linux autostart (systemd --user)

```
mkdir -p ~/.local/bin
cp dist/batt-tray-linux-cpp ~/.local/bin/batt-tray   # or -rust
cp linux/batt-tray.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now batt-tray
```

Optional flag: `batt-tray --hide-offline` sets offline devices to `Passive` status.

## CI

`.github/workflows/build.yml` builds all four binaries; pushing a `v*` tag publishes them to a GitHub Release.
