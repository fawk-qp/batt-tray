#include "tray.hpp"
#include "icon.hpp"

#include <shellapi.h>

#include <algorithm>
#include <cwchar>
#include <string>

namespace {

constexpr const wchar_t* kRunSubkey = L"Software\\Microsoft\\Windows\\CurrentVersion\\Run";
constexpr const wchar_t* kRunValue = L"BattTray";
constexpr const wchar_t* kSettingsSubkey = L"Software\\BattTray";
constexpr const wchar_t* kHideOfflineValue = L"HideOffline";
constexpr const wchar_t* kLowBatteryAlertValue = L"LowBatteryAlert";
constexpr const wchar_t* kAnchorTooltip = L"BattTray \u2014 нет подключённых устройств";

DWORD LoadDwordSetting(const wchar_t* valueName, DWORD defaultVal) {
    DWORD val = 0;
    DWORD cb = sizeof(val);
    const LONG rc = RegGetValueW(HKEY_CURRENT_USER, kSettingsSubkey, valueName,
                                 RRF_RT_REG_DWORD, nullptr, &val, &cb);
    return (rc == ERROR_SUCCESS) ? val : defaultVal;
}

void SaveDwordSetting(const wchar_t* valueName, DWORD val) {
    HKEY key = nullptr;
    if (RegCreateKeyExW(HKEY_CURRENT_USER, kSettingsSubkey, 0, nullptr, 0,
                        KEY_SET_VALUE, nullptr, &key, nullptr) != ERROR_SUCCESS) {
        return;
    }
    RegSetValueExW(key, valueName, 0, REG_DWORD, reinterpret_cast<const BYTE*>(&val), sizeof(val));
    RegCloseKey(key);
}

std::wstring TruncateName(const std::wstring& s, size_t maxLen) {
    if (s.size() <= maxLen) return s;
    return s.substr(0, maxLen - 1) + L"\u2026";
}

std::wstring FormatTooltip(const SlotSpec& spec, const DeviceState& st) {
    if (!st.online || st.items.empty()) {
        return std::wstring(spec.label) + L": не подключено";
    }
    if (st.items.size() == 1) {
        const auto& d = st.items[0];
        std::wstring tip = std::wstring(spec.label) + L": " + std::to_wstring(d.percent) + L"%";
        if (d.charging) tip += L" (зарядка)";
        if (!d.name.empty()) tip += L"\n" + d.name;
        return tip;
    }

    std::wstring tip;
    const size_t maxLines = 4;
    const size_t nameCap = (st.items.size() >= 3) ? 20u : 28u;
    for (size_t i = 0; i < st.items.size() && i < maxLines; ++i) {
        const auto& d = st.items[i];
        std::wstring label = d.name.empty() ? std::wstring(spec.label) : TruncateName(d.name, nameCap);
        std::wstring line = (d.activeAudio ? L"\u25B6 " : L"\u2022 ") + label +
                            L": " + std::to_wstring(d.percent) + L"%" +
                            (d.charging ? L" \u26A1" : L"");
        if (!tip.empty()) {
            if (tip.size() + 1 + line.size() > 126) break;
            tip += L"\n";
        }
        tip += line;
    }
    return tip;
}

std::wstring FormatMenuHeader(const SlotSpec& spec, const DeviceState& st) {
    if (!st.online || st.items.empty()) {
        return std::wstring(spec.label) + L": не подключено";
    }
    const auto& d = st.items.front();
    std::wstring title = d.name.empty() ? std::wstring(spec.label) : TruncateName(d.name, 34);
    std::wstring hdr = (st.items.size() > 1 ? L"\u25B6 " : L"") + title +
                       L" — " + std::to_wstring(d.percent) + L"%" +
                       (d.charging ? L" \u26A1" : L"");
    if (st.items.size() > 1) {
        hdr += L" (+" + std::to_wstring(st.items.size() - 1) + L")";
    }
    return hdr;
}

std::wstring GetCurrentExePath() {
    std::wstring buf(MAX_PATH, L'\0');
    for (;;) {
        DWORD len = GetModuleFileNameW(nullptr, buf.data(), static_cast<DWORD>(buf.size()));
        if (len == 0) return {};
        if (len < buf.size() - 1) {
            buf.resize(len);
            return buf;
        }
        if (buf.size() >= 32768) return {};
        buf.resize(buf.size() * 2);
    }
}

} // namespace

bool TrayManager::Init(HWND hwnd) {
    hwnd_ = hwnd;
    lightTheme_ = IsTaskbarLightTheme();
    hideOffline_ = (LoadDwordSetting(kHideOfflineValue, 0) != 0);
    lowBatteryAlert_ = (LoadDwordSetting(kLowBatteryAlertValue, 1) != 0);
    history_.Load();
    SyncVisibility(GetTaskbarIconSize());
    ShutdownIconRenderer();
    return true;
}

void TrayManager::ReaddIcons() {
    if (!hwnd_) return;
    lightTheme_ = IsTaskbarLightTheme();
    const int iconSize = GetTaskbarIconSize();
    bool online[3] = {states_[0].online, states_[1].online, states_[2].online};
    const SlotVisibility vis = ComputeSlotVisibility(online, hideOffline_, anchorSlot_);
    anchorSlot_ = vis.anchorSlot;
    const auto& specs = GetSlotSpecs();
    bool anyRendered = false;
    for (size_t i = 0; i < 3; ++i) {
        if (!vis.visible[i]) continue;
        const std::wstring tip = (vis.anchorMode && static_cast<int>(i) == vis.anchorSlot)
            ? std::wstring(kAnchorTooltip)
            : FormatTooltip(specs[i], states_[i]);
        ApplySlot(i, true, iconSize, tip);
        anyRendered = true;
    }
    if (anyRendered) ShutdownIconRenderer();
}

bool TrayManager::Update(const std::array<DeviceState, 3>& states) {
    states_ = states;
    history_.SyncActive(states_);

    const bool anyRendered = SyncVisibility(GetTaskbarIconSize());
    if (anyRendered) ShutdownIconRenderer();
    return anyRendered;
}

void TrayManager::EvaluateLowBatteryAlerts(const std::array<DeviceState, 3>& states) {
    const uint64_t nowSec = GetTickCount64() / 1000ULL;
    alertTracker_.PruneOffline(nowSec);
    if (!lowBatteryAlert_) return;

    const auto& specs = GetSlotSpecs();
    for (size_t i = 0; i < 3; ++i) {
        if (!states[i].online) continue;
        for (const auto& d : states[i].items) {
            const std::wstring devName = d.name.empty() ? std::wstring(specs[i].label) : d.name;
            const AlertLevel lvl = alertTracker_.Evaluate(static_cast<int>(i), devName, d.percent, d.charging, nowSec);
            if (lvl == AlertLevel::None) continue;
            const bool critical = (lvl == AlertLevel::Critical5);
            const std::wstring title = (critical ? L"Критический заряд: " : L"Низкий заряд: ") + devName;
            const std::wstring body = L"Осталось " + std::to_wstring(d.percent) + L"% заряда";
            ShowLowBatteryToast(i, title, body, critical);
        }
    }
}

void TrayManager::ShowLowBatteryToast(size_t slot, const std::wstring& title, const std::wstring& body, bool critical) {
    if (slot >= 3 || !hwnd_ || !addedInTray_[slot]) return;
    NOTIFYICONDATAW nid{};
    nid.cbSize = sizeof(nid);
    nid.hWnd = hwnd_;
    nid.uID = GetSlotSpecs()[slot].uid;
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = critical ? NIIF_ERROR : NIIF_WARNING;
    wcsncpy_s(nid.szInfoTitle, _countof(nid.szInfoTitle), title.c_str(), _TRUNCATE);
    wcsncpy_s(nid.szInfo, _countof(nid.szInfo), body.c_str(), _TRUNCATE);
    Shell_NotifyIconW(NIM_MODIFY, &nid);
}

bool TrayManager::RefreshTheme() {
    const bool light = IsTaskbarLightTheme();
    if (light == lightTheme_) return false;
    lightTheme_ = light;
    const bool anyRendered = SyncVisibility(GetTaskbarIconSize());
    if (anyRendered) ShutdownIconRenderer();
    return true;
}

void TrayManager::ToggleHideOffline() {
    hideOffline_ = !hideOffline_;
    SaveDwordSetting(kHideOfflineValue, hideOffline_ ? 1u : 0u);
    if (SyncVisibility(GetTaskbarIconSize())) {
        ShutdownIconRenderer();
    }
}

void TrayManager::ToggleLowBatteryAlert() {
    lowBatteryAlert_ = !lowBatteryAlert_;
    SaveDwordSetting(kLowBatteryAlertValue, lowBatteryAlert_ ? 1u : 0u);
    if (!lowBatteryAlert_) {
        alertTracker_.Clear();
    }
}

bool TrayManager::SyncVisibility(int iconSize) {
    bool online[3] = {states_[0].online, states_[1].online, states_[2].online};
    const SlotVisibility vis = ComputeSlotVisibility(online, hideOffline_, anchorSlot_);
    anchorSlot_ = vis.anchorSlot;

    const auto& specs = GetSlotSpecs();
    bool anyRendered = false;
    for (size_t i = 0; i < 3; ++i) {
        if (!vis.visible[i]) {
            if (addedInTray_[i]) {
                DeleteSlot(i);
                anyRendered = true;
            }
            continue;
        }
        const std::wstring tip = (vis.anchorMode && static_cast<int>(i) == vis.anchorSlot)
            ? std::wstring(kAnchorTooltip)
            : FormatTooltip(specs[i], states_[i]);
        const auto& c = cache_[i];
        const bool unchanged = addedInTray_[i] &&
                               c.valid &&
                               c.online == states_[i].online &&
                               c.percent == states_[i].percent &&
                               c.charging == states_[i].charging &&
                               c.lightTheme == lightTheme_ &&
                               c.iconSize == iconSize &&
                               c.tip == tip;
        if (unchanged) continue;
        ApplySlot(i, !addedInTray_[i], iconSize, tip);
        anyRendered = true;
    }
    return anyRendered;
}

void TrayManager::DeleteSlot(size_t idx) {
    if (addedInTray_[idx] && hwnd_) {
        NOTIFYICONDATAW nid{};
        nid.cbSize = sizeof(nid);
        nid.hWnd = hwnd_;
        nid.uID = GetSlotSpecs()[idx].uid;
        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
    addedInTray_[idx] = false;
    if (icons_[idx]) {
        DestroyIcon(icons_[idx]);
        icons_[idx] = nullptr;
    }
    cache_[idx] = SlotCache{};
}

void TrayManager::ApplySlot(size_t idx, bool add, int iconSize, const std::wstring& tip) {
    const auto& spec = GetSlotSpecs()[idx];
    HICON nextIcon = CreateBatteryTrayIcon(spec.glyph, states_[idx], lightTheme_);
    if (!nextIcon) return;

    NOTIFYICONDATAW nid{};
    nid.cbSize = sizeof(nid);
    nid.hWnd = hwnd_;
    nid.uID = spec.uid;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_APP_TRAY;
    nid.hIcon = nextIcon;
    wcsncpy_s(nid.szTip, _countof(nid.szTip), tip.c_str(), _TRUNCATE);

    BOOL ok = Shell_NotifyIconW(add ? NIM_ADD : NIM_MODIFY, &nid);
    if (!ok && !add) {
        ok = Shell_NotifyIconW(NIM_ADD, &nid);
    }
    if (!ok) {
        DestroyIcon(nextIcon);
        addedInTray_[idx] = false;
        if (icons_[idx]) {
            DestroyIcon(icons_[idx]);
            icons_[idx] = nullptr;
        }
        cache_[idx] = SlotCache{};
        return;
    }
    addedInTray_[idx] = true;

    if (icons_[idx]) DestroyIcon(icons_[idx]);
    icons_[idx] = nextIcon;

    cache_[idx] = {
        true,
        states_[idx].online,
        states_[idx].charging,
        lightTheme_,
        states_[idx].percent,
        iconSize,
        tip
    };
}

UINT TrayManager::ShowContextMenu(HWND hwnd, UINT uid) {
    menuSlot_ = 0;
    const auto& specs = GetSlotSpecs();
    for (size_t i = 0; i < specs.size(); ++i) {
        if (specs[i].uid == uid) {
            menuSlot_ = i;
            break;
        }
    }
    menuInactiveKeys_.clear();
    menuDisconnectAddrs_.clear();

    HMENU menu = CreatePopupMenu();
    if (!menu) return 0;

    const std::wstring header = FormatMenuHeader(specs[menuSlot_], states_[menuSlot_]);
    AppendMenuW(menu, MF_STRING | MF_GRAYED, IDM_HEADER, header.c_str());

    std::vector<const DeviceEntry*> activeBt;
    for (const auto& item : states_[menuSlot_].items) {
        if (item.bthAddr != 0) activeBt.push_back(&item);
    }
    if (activeBt.size() == 1) {
        menuDisconnectAddrs_.push_back(activeBt[0]->bthAddr);
        AppendMenuW(menu, MF_STRING, IDM_DISCONNECT_BASE, L"Отключить устройство");
    } else if (activeBt.size() > 1) {
        HMENU discMenu = CreatePopupMenu();
        if (discMenu) {
            for (const auto* d : activeBt) {
                menuDisconnectAddrs_.push_back(d->bthAddr);
                const UINT cmdId = IDM_DISCONNECT_BASE + static_cast<UINT>(menuDisconnectAddrs_.size() - 1);
                const std::wstring title = d->name.empty()
                    ? std::wstring(specs[menuSlot_].label)
                    : TruncateName(d->name, 28);
                const std::wstring row = (d->activeAudio ? L"\u25B6 " : L"\u2022 ") +
                                         title + L" — " + std::to_wstring(d->percent) + L"%";
                AppendMenuW(discMenu, MF_STRING, cmdId, row.c_str());
            }
            AppendMenuW(menu, MF_POPUP, reinterpret_cast<UINT_PTR>(discMenu), L"Отключить устройство");
        }
    }

    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);

    const auto inactive = history_.GetInactiveForKind(specs[menuSlot_].kind, states_[menuSlot_].items);
    if (!inactive.empty()) {
        HMENU histMenu = CreatePopupMenu();
        if (histMenu) {
            const size_t limit = std::min<size_t>(inactive.size(), 10);
            for (size_t j = 0; j < limit; ++j) {
                HMENU itemSub = CreatePopupMenu();
                if (!itemSub) continue;
                menuInactiveKeys_.push_back(inactive[j].key);
                const UINT delId = IDM_HIST_DEL_BASE + static_cast<UINT>(menuInactiveKeys_.size() - 1);
                AppendMenuW(itemSub, MF_STRING, delId, L"Удалить из истории");

                const std::wstring dName = inactive[j].name.empty()
                    ? std::wstring(specs[menuSlot_].label)
                    : TruncateName(inactive[j].name, 28);
                const std::wstring itemText = L"\u25CB " + dName + L" — " +
                    std::to_wstring(inactive[j].percent) + L"% (" +
                    DeviceHistory::FormatRelativeTime(inactive[j].lastSeenUnix) + L")";
                AppendMenuW(histMenu, MF_POPUP, reinterpret_cast<UINT_PTR>(itemSub), itemText.c_str());
            }
            AppendMenuW(histMenu, MF_SEPARATOR, 0, nullptr);
            AppendMenuW(histMenu, MF_STRING, IDM_HIST_CLEAR, L"Очистить историю");
            AppendMenuW(menu, MF_POPUP, reinterpret_cast<UINT_PTR>(histMenu), L"История (неактивные)");
            AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
        }
    }

    AppendMenuW(menu, MF_STRING, IDM_REFRESH, L"Обновить сейчас");
    AppendMenuW(menu, MF_STRING | (hideOffline_ ? MF_CHECKED : MF_UNCHECKED),
                IDM_HIDE_OFFLINE, L"Скрывать отключённые");
    AppendMenuW(menu, MF_STRING | (lowBatteryAlert_ ? MF_CHECKED : MF_UNCHECKED),
                IDM_LOW_BATTERY_ALERT, L"Уведомлять при <15%");
    AppendMenuW(menu, MF_STRING | (IsAutostartEnabled() ? MF_CHECKED : MF_UNCHECKED),
                IDM_AUTOSTART, L"Запускать с Windows");
    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
    AppendMenuW(menu, MF_STRING, IDM_EXIT, L"Выход");

    POINT pt{};
    GetCursorPos(&pt);
    SetForegroundWindow(hwnd);
    const UINT cmd = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RETURNCMD,
                                    pt.x, pt.y, 0, hwnd, nullptr);
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    return cmd;
}

void TrayManager::HandleMenuCommand(UINT cmdId) {
    const auto& specs = GetSlotSpecs();
    if (cmdId == IDM_HIST_CLEAR) {
        history_.ClearInactiveForKind(specs[menuSlot_].kind, states_[menuSlot_].items);
        menuInactiveKeys_.clear();
        return;
    }
    if (cmdId >= IDM_HIST_DEL_BASE && cmdId < IDM_DISCONNECT_BASE) {
        const size_t idx = static_cast<size_t>(cmdId - IDM_HIST_DEL_BASE);
        if (idx < menuInactiveKeys_.size()) {
            history_.RemoveEntry(specs[menuSlot_].kind, menuInactiveKeys_[idx]);
        }
        menuInactiveKeys_.clear();
    }
}

uint64_t TrayManager::TakeDisconnectAddr(UINT cmdId) {
    if (cmdId < IDM_DISCONNECT_BASE) return 0;
    const size_t idx = static_cast<size_t>(cmdId - IDM_DISCONNECT_BASE);
    uint64_t addr = (idx < menuDisconnectAddrs_.size()) ? menuDisconnectAddrs_[idx] : 0;
    menuDisconnectAddrs_.clear();
    return addr;
}

void TrayManager::Shutdown() {
    for (size_t i = 0; i < 3; ++i) {
        DeleteSlot(i);
    }
    hwnd_ = nullptr;
}

bool TrayManager::IsAutostartEnabled() {
    wchar_t buf[1024]{};
    DWORD cb = sizeof(buf);
    const LONG rc = RegGetValueW(HKEY_CURRENT_USER, kRunSubkey, kRunValue, RRF_RT_REG_SZ, nullptr, buf, &cb);
    if (rc != ERROR_SUCCESS) return false;

    std::wstring val(buf);
    while (!val.empty() && (val.back() == L'"' || val.back() == L' ')) val.pop_back();
    size_t start = 0;
    while (start < val.size() && (val[start] == L'"' || val[start] == L' ')) ++start;
    val.erase(0, start);

    const std::wstring exe = GetCurrentExePath();
    return !exe.empty() && _wcsicmp(val.c_str(), exe.c_str()) == 0;
}

bool TrayManager::ToggleAutostart() {
    HKEY key = nullptr;
    if (RegOpenKeyExW(HKEY_CURRENT_USER, kRunSubkey, 0, KEY_SET_VALUE, &key) != ERROR_SUCCESS) {
        return false;
    }

    bool ok = false;
    if (IsAutostartEnabled()) {
        ok = (RegDeleteValueW(key, kRunValue) == ERROR_SUCCESS);
    } else {
        const std::wstring exe = GetCurrentExePath();
        if (!exe.empty()) {
            const std::wstring quoted = L"\"" + exe + L"\"";
            const DWORD cb = static_cast<DWORD>((quoted.size() + 1) * sizeof(wchar_t));
            ok = (RegSetValueExW(key, kRunValue, 0, REG_SZ,
                    reinterpret_cast<const BYTE*>(quoted.c_str()), cb) == ERROR_SUCCESS);
        }
    }
    RegCloseKey(key);
    return ok;
}