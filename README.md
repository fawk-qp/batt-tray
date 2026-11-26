# batt-tray

Small tray utility that shows battery level for wireless mice, keyboards and headphones. One tray icon per device kind; right-click for refresh, autostart and exit.

- Windows: `NOTIFYICONDATA` icons drawn with GDI. Battery comes from PnP `DEVPKEY_Bluetooth_Battery`, BLE GATT (`0x180F` / `0x2A19`), and 2.4 GHz vendor HID protocols (Logitech HID++ 2.0, Razer, CompX/VGN).
- Linux: `org.kde.StatusNotifierItem` + `com.canonical.dbusmenu` over raw libdbus, no GTK/Qt. Works on KDE, GNOME AppIndicator, Waybar/Hyprland/Sway, XFCE. Supports UPower, BlueZ, `hidpp_battery_*`, and Apple Continuity BLE (`0x004C`).

Hiding empty slots on Windows can be toggled from the tray menu (keeps one anchor icon when nothing is connected), and low-battery alerts (`<=15%` / `<=5%`) can be muted per session or in the menu. Disconnected devices stay in a small history file (`~/.local/state/batt-tray/history.tsv` on Linux, `%LOCALAPPDATA%\BattTray` on Windows) and can be dropped from the tray menu. It is plaintext, you can just delete it.

## Install (Windows Scoop)

```
scoop bucket add batt-tray https://github.com/fawk-qp/batt-tray
scoop install batt-tray/batt-tray
```

## Build

Windows (MSVC + Rust):

```
build.bat
```

Linux:

```
sudo apt install g++ pkg-config libdbus-1-dev   # cargo for the Rust build
./linux/build.sh [output-dir]
```

Binaries land in `dist/`.

## Linux autostart

```
mkdir -p ~/.local/bin
cp dist/batt-tray-linux-cpp ~/.local/bin/batt-tray   # or batt-tray-linux-rust
cp linux/batt-tray.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now batt-tray
```

Flags: `--hide-offline` marks disconnected devices as passive instead of dropping them, `--no-notify` disables low-battery desktop notifications, `--airpods-ble` enables un-paired Apple Continuity BLE battery beacons on Linux, `--demo` runs on synthetic data without touching hardware or D-Bus.

## Releases

Pushing a `v*` tag builds all four binaries and publishes them with `SHA256SUMS.txt` plus a build provenance attestation. Check a download with:

```
sha256sum -c SHA256SUMS.txt
gh attestation verify batt-tray-win-rust.exe -R fawk-qp/batt-tray
```

`gh attestation verify` is the part that actually proves where the binary came from.
