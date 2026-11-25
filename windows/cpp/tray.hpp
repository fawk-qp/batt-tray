#pragma once

#include "device.hpp"
#include "history.hpp"

#include <array>
#include <string>
#include <vector>

constexpr UINT WM_APP_TRAY = WM_APP + 1;
constexpr UINT WM_APP_SCAN_DONE = WM_APP + 2;

constexpr UINT IDM_HEADER = 2000;
constexpr UINT IDM_REFRESH = 2001;
constexpr UINT IDM_AUTOSTART = 2002;
constexpr UINT IDM_EXIT = 2003;
constexpr UINT IDM_HIST_CLEAR = 2010;
constexpr UINT IDM_HIST_DEL_BASE = 2100;

class TrayManager {
public:
    bool Init(HWND hwnd);
    void ReaddIcons();
    bool Update(const std::array<DeviceState, 3>& states);
    void RefreshTheme();
    void ShowContextMenu(HWND hwnd, UINT uid);
    void HandleMenuCommand(UINT cmdId);
    void Shutdown();

    static bool IsAutostartEnabled();
    static bool ToggleAutostart();

private:
    struct SlotCache {
        bool valid = false;
        bool online = false;
        bool charging = false;
        bool lightTheme = false;
        int percent = -1;
        int iconSize = 0;
        std::wstring tip;
    };

    void ApplySlot(size_t idx, bool add, int iconSize);

    HWND hwnd_ = nullptr;
    bool lightTheme_ = false;
    DeviceHistory history_;
    std::array<DeviceState, 3> states_{};
    std::array<HICON, 3> icons_{nullptr, nullptr, nullptr};
    std::array<SlotCache, 3> cache_{};

    size_t menuSlot_ = 0;
    std::vector<std::wstring> menuInactiveKeys_;
};