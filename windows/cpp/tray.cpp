#include "tray.hpp"
#include "icon.hpp"

#include <shellapi.h>

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

    // Multiple connected devices: active audio endpoint first (marked with ▶), fit within 127 chars
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
    for (size_t i = 0; i < 3; ++i) {
        ApplySlot(i, true);
    }
    return true;
}

void TrayManager::ReaddIcons() {
    if (!hwnd_) return;
    lightTheme_ = IsTaskbarLightTheme();
    for (size_t i = 0; i < 3; ++i) {
        ApplySlot(i, true);
    }
}

void TrayManager::Update(const std::array<DeviceState, 3>& states) {
    states_ = states;
    for (size_t i = 0; i < 3; ++i) {
        ApplySlot(i, false);
    }
}

void TrayManager::RefreshTheme() {
    const bool light = IsTaskbarLightTheme();
    if (light == lightTheme_) return;
    lightTheme_ = light;
    for (size_t i = 0; i < 3; ++i) {
        ApplySlot(i, false);
    }
}

void TrayManager::ApplySlot(size_t idx, bool add) {
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

    const std::wstring tip = FormatTooltip(spec, states_[idx]);
    wcsncpy_s(nid.szTip, _countof(nid.szTip), tip.c_str(), _TRUNCATE);

    if (!Shell_NotifyIconW(add ? NIM_ADD : NIM_MODIFY, &nid) && !add) {
        Shell_NotifyIconW(NIM_ADD, &nid);
    }

    if (icons_[idx]) DestroyIcon(icons_[idx]);
    icons_[idx] = nextIcon;
}

void TrayManager::ShowContextMenu(HWND hwnd) const {
    HMENU menu = CreatePopupMenu();
    if (!menu) return;

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