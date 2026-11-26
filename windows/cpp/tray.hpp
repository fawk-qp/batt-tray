#pragma once

#include "alert_tracker.hpp"
#include "device.hpp"
#include "history.hpp"

#include <array>
#include <cstdint>
#include <string>
#include <vector>

constexpr UINT WM_APP_TRAY = WM_APP + 1;
constexpr UINT WM_APP_SCAN_DONE = WM_APP + 2;

constexpr UINT IDM_HEADER = 2000;
constexpr UINT IDM_REFRESH = 2001;
constexpr UINT IDM_AUTOSTART = 2002;
constexpr UINT IDM_EXIT = 2003;
constexpr UINT IDM_HIDE_OFFLINE = 2004;
constexpr UINT IDM_LOW_BATTERY_ALERT = 2005;
constexpr UINT IDM_HIST_CLEAR = 2010;
constexpr UINT IDM_HIST_DEL_BASE = 2100;
constexpr UINT IDM_DISCONNECT_BASE = 2200;

struct SlotVisibility {
    bool visible[3] = {true, true, true};
    bool anchorMode = false;
    int anchorSlot = 2;
};

inline SlotVisibility ComputeSlotVisibility(const bool online[3], bool hideOffline, int lastAnchor) {
    SlotVisibility v;
    v.anchorSlot = (lastAnchor >= 0 && lastAnchor < 3) ? lastAnchor : 2;
    if (!hideOffline) {
        return v;
    }
    int onlineCount = 0;
    int firstOnline = -1;
    for (int i = 0; i < 3; ++i) {
        v.visible[i] = online[i];
        if (online[i]) {
            ++onlineCount;
            if (firstOnline < 0) firstOnline = i;
        }
    }
    if (onlineCount > 0) {
        if (!online[v.anchorSlot]) v.anchorSlot = firstOnline;
        return v;
    }
    v.anchorMode = true;
    v.visible[0] = v.visible[1] = v.visible[2] = false;
    v.visible[v.anchorSlot] = true;
    return v;
}

class TrayManager {
public:
    bool Init(HWND hwnd);
    void ReaddIcons();
    bool Update(const std::array<DeviceState, 3>& states);
    void EvaluateLowBatteryAlerts(const std::array<DeviceState, 3>& states);
    void ShowLowBatteryToast(size_t slot, const std::wstring& title, const std::wstring& body, bool critical);
    bool RefreshTheme();
    UINT ShowContextMenu(HWND hwnd, UINT uid);
    void HandleMenuCommand(UINT cmdId);
    void ToggleHideOffline();
    void ToggleLowBatteryAlert();
    uint64_t TakeDisconnectAddr(UINT cmdId);
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

    bool SyncVisibility(int iconSize);
    void ApplySlot(size_t idx, bool add, int iconSize, const std::wstring& tip);
    void DeleteSlot(size_t idx);

    HWND hwnd_ = nullptr;
    bool lightTheme_ = false;
    bool hideOffline_ = false;
    bool lowBatteryAlert_ = true;
    int anchorSlot_ = 2;
    bool addedInTray_[3]{false, false, false};
    DeviceHistory history_;
    AlertTracker alertTracker_;
    std::array<DeviceState, 3> states_{};
    std::array<HICON, 3> icons_{nullptr, nullptr, nullptr};
    std::array<SlotCache, 3> cache_{};

    size_t menuSlot_ = 0;
    std::vector<std::wstring> menuInactiveKeys_;
    std::vector<uint64_t> menuDisconnectAddrs_;
};