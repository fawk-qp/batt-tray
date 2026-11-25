#include "tray.hpp"
#include "icon.hpp"

#include <shellapi.h>

#include <algorithm>
#include <cwchar>
#include <string>

namespace {

constexpr const wchar_t* kRunSubkey = L"Software\\Microsoft\\Windows\\CurrentVersion\\Run";
constexpr const wchar_t* kRunValue = L"BattTray";

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
    history_.Load();
    const int iconSize = GetTaskbarIconSize();
    for (size_t i = 0; i < 3; ++i) {
        ApplySlot(i, true, iconSize);
    }
    ShutdownIconRenderer();
    return true;
}

void TrayManager::ReaddIcons() {
    if (!hwnd_) return;
    lightTheme_ = IsTaskbarLightTheme();
    const int iconSize = GetTaskbarIconSize();
    for (size_t i = 0; i < 3; ++i) {
        ApplySlot(i, true, iconSize);
    }
    ShutdownIconRenderer();
}

bool TrayManager::Update(const std::array<DeviceState, 3>& states) {
    states_ = states;
    history_.SyncActive(states_);

    const int iconSize = GetTaskbarIconSize();
    bool anyRendered = false;
    for (size_t i = 0; i < 3; ++i) {
        const auto& spec = GetSlotSpecs()[i];
        const std::wstring tip = FormatTooltip(spec, states_[i]);
        const auto& c = cache_[i];
        const bool unchanged = c.valid &&
                               c.online == states_[i].online &&
                               c.percent == states_[i].percent &&
                               c.charging == states_[i].charging &&
                               c.lightTheme == lightTheme_ &&
                               c.iconSize == iconSize &&
                               c.tip == tip;
        if (unchanged) continue;
        ApplySlot(i, false, iconSize);
        anyRendered = true;
    }
    if (anyRendered) ShutdownIconRenderer();
    return anyRendered;
}

void TrayManager::RefreshTheme() {
    const bool light = IsTaskbarLightTheme();
    if (light == lightTheme_) return;
    lightTheme_ = light;
    const int iconSize = GetTaskbarIconSize();
    for (size_t i = 0; i < 3; ++i) {
        ApplySlot(i, false, iconSize);
    }
    ShutdownIconRenderer();
}

void TrayManager::ApplySlot(size_t idx, bool add, int iconSize) {
    const auto& spec = GetSlotSpecs()[idx];
    HICON nextIcon = CreateBatteryTrayIcon(spec.glyph, states_[idx], lightTheme_);
    if (!nextIcon) return;

    const std::wstring tip = FormatTooltip(spec, states_[idx]);
    NOTIFYICONDATAW nid{};
    nid.cbSize = sizeof(nid);
    nid.hWnd = hwnd_;
    nid.uID = spec.uid;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_APP_TRAY;
    nid.hIcon = nextIcon;
    wcsncpy_s(nid.szTip, _countof(nid.szTip), tip.c_str(), _TRUNCATE);

    if (!Shell_NotifyIconW(add ? NIM_ADD : NIM_MODIFY, &nid) && !add) {
        Shell_NotifyIconW(NIM_ADD, &nid);
    }

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

void TrayManager::ShowContextMenu(HWND hwnd, UINT uid) {
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
    if (!menu) return;

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
    AppendMenuW(menu, MF_STRING | (IsAutostartEnabled() ? MF_CHECKED : MF_UNCHECKED),
                IDM_AUTOSTART, L"Запускать с Windows");
    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
    AppendMenuW(menu, MF_STRING, IDM_EXIT, L"Выход");

    POINT pt{};
    GetCursorPos(&pt);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN, pt.x, pt.y, 0, hwnd, nullptr);
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
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
        if (hwnd_) {
            NOTIFYICONDATAW nid{};
            nid.cbSize = sizeof(nid);
            nid.hWnd = hwnd_;
            nid.uID = GetSlotSpecs()[i].uid;
            Shell_NotifyIconW(NIM_DELETE, &nid);
        }
        if (icons_[i]) {
            DestroyIcon(icons_[i]);
            icons_[i] = nullptr;
        }
    }
    hwnd_ = nullptr;
}

bool TrayManager::IsAutostartEnabled() {
    DWORD type = 0;
    DWORD cb = 0;
    const LONG rc = RegGetValueW(HKEY_CURRENT_USER, kRunSubkey, kRunValue, RRF_RT_REG_SZ, &type, nullptr, &cb);
    return rc == ERROR_SUCCESS && cb > sizeof(wchar_t);
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