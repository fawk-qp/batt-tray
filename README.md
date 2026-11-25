# batt-tray

Tray icons showing battery level for wireless mice, keyboards and headphones. One icon per device kind, tooltip on hover, right-click menu for refresh / autostart / exit.

- Windows: native `NOTIFYICONDATA` icons rendered via GDI/Direct2D, battery read from PnP `DEVPKEY_Bluetooth_Battery` with a BLE GATT (`0x180F`/`0x2A19`) fallback.
- Linux: `org.kde.StatusNotifierItem` + `com.canonical.dbusmenu` over raw libdbus — no GTK/Qt. Works on KDE Plasma, GNOME AppIndicator, Waybar/Hyprland/Sway, XFCE.

Devices that drop off are kept in a small on-disk history (`~/.local/state/batt-tray/history.tsv` on Linux, `%LOCALAPPDATA%\BattTray` on Windows) and can be removed from the tray menu.

## Platforms and binaries

| Platform | Language | Output |
|---|---|---|
| Windows | C++17 (MSVC) | `batt-tray-win-cpp.exe` |
| Windows | Rust, no dependencies | `batt-tray-win-rust.exe` |
| Linux | C++17 (libdbus-1) | `batt-tray-linux-cpp` |
| Linux | Rust, no crates, raw libdbus FFI | `batt-tray-linux-rust` |

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

Flags: `--hide-offline` marks disconnected devices `Passive` instead of `Active`; `--demo` runs with synthetic devices and a demo history (no hardware or D-Bus session needed).

## Releases

`.github/workflows/build.yml` builds all four binaries on every push; a `v*` tag publishes them as a GitHub Release together with `SHA256SUMS.txt` and a build-provenance attestation.

Verify a download:

```
sha256sum -c SHA256SUMS.txt
gh attestation verify batt-tray-win-rust.exe -R fawk-qp/batt-tray
```
