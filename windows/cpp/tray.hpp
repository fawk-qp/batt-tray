#pragma once

#include "device.hpp"

#include <array>

constexpr UINT WM_APP_TRAY = WM_APP + 1;
constexpr UINT WM_APP_SCAN_DONE = WM_APP + 2;

constexpr UINT IDM_REFRESH = 2001;
constexpr UINT IDM_AUTOSTART = 2002;
constexpr UINT IDM_EXIT = 2003;

class TrayManager {
public:
    bool Init(HWND hwnd);
    void ReaddIcons();
    void Update(const std::array<DeviceState, 3>& states);
    void RefreshTheme();
    void ShowContextMenu(HWND hwnd) const;
    void Shutdown();

    static bool IsAutostartEnabled();
    static bool ToggleAutostart();

private:
    void ApplySlot(size_t idx, bool add);

    HWND hwnd_ = nullptr;
    bool lightTheme_ = false;
    std::array<DeviceState, 3> states_{};
    std::array<HICON, 3> icons_{nullptr, nullptr, nullptr};
};