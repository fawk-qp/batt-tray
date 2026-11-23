#define _WIN32_WINNT 0x0A00
#define NOMINMAX

#include <windows.h>
#include <shellapi.h>
#include <setupapi.h>
#include <devpropdef.h>
#include <dbt.h>
#include <bluetoothleapis.h>
extern "C" {
#include <hidsdi.h>
}

#include <string>
#include <vector>
#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cwchar>
#include <cwctype>
#include <cstddef>
#include <iterator>

#pragma comment(lib, "user32.lib")
#pragma comment(lib, "gdi32.lib")
#pragma comment(lib, "shell32.lib")
#pragma comment(lib, "advapi32.lib")
#pragma comment(lib, "setupapi.lib")
#pragma comment(lib, "hid.lib")

static constexpr UINT WM_TRAY        = WM_APP + 1;
static constexpr UINT WM_UPDATE_UI   = WM_APP + 2;
static constexpr UINT UID_MOUSE      = 1;
static constexpr UINT UID_HEADPHONES = 2;
static constexpr UINT_PTR TIMER_DEBOUNCE = 101;
static constexpr DWORD POLL_INTERVAL_MS  = 60000;

static constexpr DEVPROPKEY PKEY_BT_BATTERY = {
    {0x104EA319, 0x6EE2, 0x4701, {0xBD, 0x47, 0x8D, 0xDB, 0xF4, 0x25, 0xBB, 0xE5}}, 2
};
static constexpr DEVPROPKEY PKEY_BT_COD = {
    {0x2BD67D8B, 0x8BEB, 0x48D5, {0x87, 0xE0, 0x6C, 0xDA, 0x34, 0x28, 0x04, 0x0A}}, 10
};
static constexpr DEVPROPKEY PKEY_FRIENDLY_NAME = {
    {0xA45C254E, 0xDF1C, 0x4EFD, {0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0}}, 14
};
static constexpr DEVPROPKEY PKEY_DEVICE_DESC = {
    {0xA45C254E, 0xDF1C, 0x4EFD, {0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0}}, 2
};
static constexpr DEVPROPKEY PKEY_DEVICE_CLASS = {
    {0xA45C254E, 0xDF1C, 0x4EFD, {0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0}}, 9
};
static constexpr DEVPROPKEY PKEY_DEVNODE_STATUS = {
    {0x4340A6C5, 0x93FA, 0x4706, {0x97, 0x2C, 0x7B, 0x64, 0x80, 0x08, 0xA5, 0xA7}}, 2
};

enum class DeviceKind { Unknown, Mouse, Headphones };

struct DeviceState {
    std::wstring name;
    int  percent  = -1;
    bool online   = false;
    bool started  = false;
};

struct LockGuard {
    CRITICAL_SECTION& cs;
    explicit LockGuard(CRITICAL_SECTION& c) : cs(c) { EnterCriticalSection(&cs); }
    ~LockGuard() { LeaveCriticalSection(&cs); }
};

static CRITICAL_SECTION g_cs;
static DeviceState g_mouse, g_headphones;
static HWND g_hwnd = nullptr;
static HANDLE g_stopEv = nullptr, g_refreshEv = nullptr;
static UINT g_taskbarCreated = 0;

struct TraySlot {
    NOTIFYICONDATAW nid{};
    HICON icon = nullptr;
    bool added = false;
};
static TraySlot g_slotMouse, g_slotHeadphones;

static std::wstring ToLower(std::wstring s) {
    for (auto& c : s) c = (wchar_t)towlower(c);
    return s;
}

static bool ContainsAny(const std::wstring& hay, std::initializer_list<const wchar_t*> needles) {
    for (const wchar_t* n : needles) {
        if (hay.find(n) != std::wstring::npos) return true;
    }
    return false;
}

static std::wstring CleanName(std::wstring name) {
    const wchar_t* suffixes[] = {
        L" Hands-Free AG Audio", L" Hands-Free Audio", L" Hands-Free",
        L" Stereo", L" Avrcp Transport", L" Headphone", L" Headset"
    };
    for (const wchar_t* sfx : suffixes) {
        size_t pos = name.find(sfx);
        if (pos != std::wstring::npos && pos > 2) {
            name.erase(pos);
        }
    }
    while (!name.empty() && name.back() == L' ') name.pop_back();
    return name;
}

static DeviceKind ClassifyDevice(const std::wstring& name, const std::wstring& devClass, uint32_t cod) {
    if (cod != 0) {
        // Bluetooth Class of Device major-class and pointing-device bits.
        uint32_t major = (cod >> 8) & 0x1Fu;
        if (major == 0x04u) return DeviceKind::Headphones;
        if (major == 0x05u && (cod & 0x80u)) return DeviceKind::Mouse;
    }

    const std::wstring low = ToLower(name);
    const std::wstring cls = ToLower(devClass);

    if (ContainsAny(low, {L"keyboard", L"клавиатур", L"gamepad", L"controller", L"xbox", L"dualsense"}))
        return DeviceKind::Unknown;

    if (ContainsAny(low, {
            L"mouse", L"мышь", L"мыш", L"mx master", L"mx anywhere", L"g pro", L"superlight",
            L"g305", L"g304", L"viper", L"deathadder", L"basilisk", L"orochi", L"aerox",
            L"lamzu", L"pulsar", L"attack shark", L"vgn", L"m720", L"m650", L"pebble", L"trackball"}))
        return DeviceKind::Mouse;

    if (ContainsAny(low, {
            L"headphone", L"headset", L"earbud", L"earphone", L"airpods", L"buds", L"pods",
            L"hands-free", L"audio", L"наушник", L"гарнитур", L"wh-1000", L"wf-1000", L"jbl",
            L"marshall", L"bose", L"qc35", L"qc45", L"sennheiser", L"momentum", L"beats",
            L"barracuda", L"blackshark", L"arctis", L"cloud", L"g733", L"g435", L"freebuds",
            L"soundcore", L"anker", L"edifier", L"sony", L"major ", L"minor "}))
        return DeviceKind::Headphones;

    if (cls == L"mouse") return DeviceKind::Mouse;
    if (cls == L"audioendpoint" || cls == L"media") return DeviceKind::Headphones;
    return DeviceKind::Unknown;
}

static bool IsLightTaskbar() {
    HKEY k = nullptr;
    DWORD val = 0, sz = sizeof(val);
    if (RegOpenKeyExW(HKEY_CURRENT_USER,
            L"Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
            0, KEY_QUERY_VALUE, &k) == ERROR_SUCCESS) {
        RegQueryValueExW(k, L"SystemUsesLightTheme", nullptr, nullptr, (LPBYTE)&val, &sz);
        RegCloseKey(k);
    }
    return val != 0;
}

static const wchar_t* PickIconFont() {
    static const wchar_t* fontName = nullptr;
    if (!fontName) {
        wchar_t windowsDir[MAX_PATH]{};
        const UINT len = GetWindowsDirectoryW(windowsDir, MAX_PATH);
        std::wstring fontPath;
        if (len > 0 && len < MAX_PATH) {
            fontPath.assign(windowsDir, len);
            fontPath += L"\\Fonts\\SegoeIcons.ttf";
        }
        fontName = (!fontPath.empty() && GetFileAttributesW(fontPath.c_str()) != INVALID_FILE_ATTRIBUTES)
                   ? L"Segoe Fluent Icons" : L"Segoe MDL2 Assets";
    }
    return fontName;
}

static HICON CreateNativeIcon(wchar_t glyph, int percent, bool online) {
    int size = GetSystemMetrics(SM_CXSMICON);
    if (size < 16) size = 16;

    const bool lightTheme = IsLightTaskbar();
    const uint32_t fgR = lightTheme ? 24 : 255;
    const uint32_t fgG = lightTheme ? 24 : 255;
    const uint32_t fgB = lightTheme ? 24 : 255;

    BITMAPINFO bi{};
    bi.bmiHeader.biSize        = sizeof(BITMAPINFOHEADER);
    bi.bmiHeader.biWidth       = size;
    bi.bmiHeader.biHeight      = -size; // top-down
    bi.bmiHeader.biPlanes      = 1;
    bi.bmiHeader.biBitCount    = 32;
    bi.bmiHeader.biCompression = BI_RGB;

    void* bits = nullptr;
    HDC screen = GetDC(nullptr);
    if (!screen) return nullptr;
    HBITMAP bmp = CreateDIBSection(screen, &bi, DIB_RGB_COLORS, &bits, nullptr, 0);
    if (!bmp || !bits) {
        if (bmp) DeleteObject(bmp);
        ReleaseDC(nullptr, screen);
        return nullptr;
    }
    HDC dc = CreateCompatibleDC(screen);
    if (!dc) {
        DeleteObject(bmp);
        ReleaseDC(nullptr, screen);
        return nullptr;
    }
    HGDIOBJ oldBmp = SelectObject(dc, bmp);
    if (!oldBmp || oldBmp == HGDI_ERROR) {
        DeleteDC(dc);
        DeleteObject(bmp);
        ReleaseDC(nullptr, screen);
        return nullptr;
    }

    RECT fullRc{0, 0, size, size};
    FillRect(dc, &fullRc, (HBRUSH)GetStockObject(BLACK_BRUSH));
    SetBkMode(dc, TRANSPARENT);

    const int barH = std::max(2, size / 8);
    const int glyphH = size - (online ? (barH + 1) : 0);
    HFONT font = CreateFontW(-glyphH, 0, 0, 0, FW_NORMAL, 0, 0, 0, DEFAULT_CHARSET,
                             OUT_TT_PRECIS, CLIP_DEFAULT_PRECIS, ANTIALIASED_QUALITY,
                             DEFAULT_PITCH, PickIconFont());
    if (!font) {
        SelectObject(dc, oldBmp);
        DeleteDC(dc);
        DeleteObject(bmp);
        ReleaseDC(nullptr, screen);
        return nullptr;
    }
    HGDIOBJ oldFont = SelectObject(dc, font);
    if (!oldFont || oldFont == HGDI_ERROR) {
        DeleteObject(font);
        SelectObject(dc, oldBmp);
        DeleteDC(dc);
        DeleteObject(bmp);
        ReleaseDC(nullptr, screen);
        return nullptr;
    }
    SetTextColor(dc, RGB(255, 255, 255));

    wchar_t str[2] = {glyph, 0};
    RECT glyphRc{0, 0, size, glyphH};
    DrawTextW(dc, str, 1, &glyphRc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);

    SelectObject(dc, oldFont);
    DeleteObject(font);
    GdiFlush();

    auto* px = static_cast<uint32_t*>(bits);
    // Icon pixels use premultiplied BGRA.
    const uint32_t opacityMul = online ? 255u : 110u;
    for (int i = 0; i < size * size; ++i) {
        uint32_t lum = px[i] & 0xFFu;
        uint32_t a = (lum * opacityMul) / 255u;
        uint32_t pr = (fgR * a) / 255u;
        uint32_t pg = (fgG * a) / 255u;
        uint32_t pb = (fgB * a) / 255u;
        px[i] = (a << 24) | (pr << 16) | (pg << 8) | pb;
    }

    if (online && percent >= 0) {
        uint32_t r = 60, g = 215, b = 110;
        if (percent <= 20)      { r = 255; g = 75;  b = 75; }
        else if (percent <= 40) { r = 255; g = 185; b = 45; }

        const uint32_t fillCol = 0xFF000000u | (r << 16) | (g << 8) | b;
        const uint32_t bgCol   = lightTheme ? 0x44000000u : 0x44FFFFFFu;
        const int pad = 1;
        const int trackW = std::max(1, size - pad * 2);
        const int fillW  = std::clamp((trackW * percent + 50) / 100, 1, trackW);

        for (int y = size - barH; y < size; ++y) {
            for (int x = pad; x < size - pad; ++x) {
                px[y * size + x] = ((x - pad) < fillW) ? fillCol : bgCol;
            }
        }
    }

    std::vector<BYTE> mask(((size + 15) / 16) * 2 * size, 0);
    HBITMAP hMask = CreateBitmap(size, size, 1, 1, mask.data());
    if (!hMask) {
        SelectObject(dc, oldBmp);
        DeleteDC(dc);
        DeleteObject(bmp);
        ReleaseDC(nullptr, screen);
        return nullptr;
    }
    ICONINFO ii{};
    ii.fIcon    = TRUE;
    ii.hbmColor = bmp;
    ii.hbmMask  = hMask;
    HICON icon  = CreateIconIndirect(&ii);

    SelectObject(dc, oldBmp);
    DeleteDC(dc);
    DeleteObject(bmp);
    DeleteObject(hMask);
    ReleaseDC(nullptr, screen);
    return icon;
}

static bool ReadPropString(HDEVINFO set, SP_DEVINFO_DATA* dev, const DEVPROPKEY& key, std::wstring& out) {
    DEVPROPTYPE type = 0;
    DWORD needed = 0;
    SetupDiGetDevicePropertyW(set, dev, &key, &type, nullptr, 0, &needed, 0);
    if (needed < sizeof(wchar_t) || needed % sizeof(wchar_t) != 0 || type != 0x00000012u) return false;
    std::wstring buf(needed / sizeof(wchar_t), L'\0');
    if (!SetupDiGetDevicePropertyW(set, dev, &key, &type, (PBYTE)buf.data(), needed, nullptr, 0)) return false;
    while (!buf.empty() && buf.back() == L'\0') buf.pop_back();
    out = std::move(buf);
    return !out.empty();
}

static void UpdateCandidate(DeviceState& slot, const std::wstring& name, int pct, bool started) {
    if (!slot.online || (started && !slot.started)) {
        slot.name    = CleanName(name);
        slot.percent = pct;
        slot.online  = true;
        slot.started = started;
    }
}

static void ScanPnPBluetooth(DeviceState& mouse, DeviceState& headphones) {
    HDEVINFO set = SetupDiGetClassDevsW(nullptr, nullptr, nullptr, DIGCF_ALLCLASSES | DIGCF_PRESENT);
    if (set == INVALID_HANDLE_VALUE) return;

    SP_DEVINFO_DATA dev{};
    dev.cbSize = sizeof(dev);
    for (DWORD i = 0; SetupDiEnumDeviceInfo(set, i, &dev); ++i) {
        DEVPROPTYPE type = 0;
        BYTE bat = 0;
        if (!SetupDiGetDevicePropertyW(set, &dev, &PKEY_BT_BATTERY, &type, &bat, sizeof(bat), nullptr, 0))
            continue;
        if (type != 0x00000003u || bat > 100) continue;

        uint32_t status = 0;
        bool started = true;
        if (SetupDiGetDevicePropertyW(set, &dev, &PKEY_DEVNODE_STATUS, &type,
                (PBYTE)&status, sizeof(status), nullptr, 0) && type == 0x00000007u) {
            started = (status & 0x00000008u) != 0;
        }

        uint32_t cod = 0;
        SetupDiGetDevicePropertyW(set, &dev, &PKEY_BT_COD, &type, (PBYTE)&cod, sizeof(cod), nullptr, 0);

        std::wstring name, cls;
        if (!ReadPropString(set, &dev, PKEY_FRIENDLY_NAME, name))
            ReadPropString(set, &dev, PKEY_DEVICE_DESC, name);
        ReadPropString(set, &dev, PKEY_DEVICE_CLASS, cls);

        DeviceKind kind = ClassifyDevice(name, cls, cod);
        if (kind == DeviceKind::Mouse)
            UpdateCandidate(mouse, name.empty() ? L"Bluetooth Mouse" : name, (int)bat, started);
        else if (kind == DeviceKind::Headphones)
            UpdateCandidate(headphones, name.empty() ? L"Bluetooth Headphones" : name, (int)bat, started);
    }
    SetupDiDestroyDeviceInfoList(set);
}

using PFN_GattGetChars = HRESULT (WINAPI*)(HANDLE, PBTH_LE_GATT_SERVICE, USHORT, PBTH_LE_GATT_CHARACTERISTIC, PUSHORT, ULONG);
using PFN_GattGetVal   = HRESULT (WINAPI*)(HANDLE, PBTH_LE_GATT_CHARACTERISTIC, ULONG, PBTH_LE_GATT_CHARACTERISTIC_VALUE, PUSHORT, ULONG);

static void ScanBleGattFallback(DeviceState& mouse, DeviceState& headphones) {
    if (mouse.online && headphones.online) return;
    HMODULE hBt = LoadLibraryExW(L"BluetoothAPIs.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32);
    if (!hBt) return;

    auto pGetChars = (PFN_GattGetChars)GetProcAddress(hBt, "BluetoothGATTGetCharacteristics");
    auto pGetVal   = (PFN_GattGetVal)GetProcAddress(hBt, "BluetoothGATTGetCharacteristicValue");
    if (!pGetChars || !pGetVal) { FreeLibrary(hBt); return; }

    GUID batSvc = {0x0000180F, 0x0000, 0x1000, {0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB}};
    HDEVINFO set = SetupDiGetClassDevsW(&batSvc, nullptr, nullptr, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
    if (set != INVALID_HANDLE_VALUE) {
        SP_DEVICE_INTERFACE_DATA ifd{};
        ifd.cbSize = sizeof(ifd);
        for (DWORD i = 0; SetupDiEnumDeviceInterfaces(set, nullptr, &batSvc, i, &ifd); ++i) {
            DWORD req = 0;
            SetupDiGetDeviceInterfaceDetailW(set, &ifd, nullptr, 0, &req, nullptr);
            if (req < offsetof(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath) + sizeof(wchar_t)) continue;

            std::vector<ULONG_PTR> storage((static_cast<size_t>(req) + sizeof(ULONG_PTR) - 1) / sizeof(ULONG_PTR));
            auto* buf = reinterpret_cast<BYTE*>(storage.data());
            auto* det = reinterpret_cast<PSP_DEVICE_INTERFACE_DETAIL_DATA_W>(buf);
            det->cbSize = sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA_W);
            SP_DEVINFO_DATA di{}; di.cbSize = sizeof(di);
            if (!SetupDiGetDeviceInterfaceDetailW(set, &ifd, det, req, nullptr, &di)) continue;
            const size_t pathChars = (req - offsetof(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath)) / sizeof(wchar_t);
            if (std::find(det->DevicePath, det->DevicePath + pathChars, L'\0') == det->DevicePath + pathChars) continue;

            wchar_t nm[256] = L"BLE Device";
            SetupDiGetDeviceRegistryPropertyW(set, &di, SPDRP_FRIENDLYNAME, nullptr, (PBYTE)nm, sizeof(nm), nullptr);
            nm[std::size(nm) - 1] = L'\0';

            HANDLE hSvc = CreateFileW(det->DevicePath, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                      nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
            if (hSvc == INVALID_HANDLE_VALUE) continue;

            USHORT cnt = 0;
            pGetChars(hSvc, nullptr, 0, nullptr, &cnt, BLUETOOTH_GATT_FLAG_NONE);
            if (cnt > 0) {
                std::vector<BTH_LE_GATT_CHARACTERISTIC> chars(cnt);
                if (SUCCEEDED(pGetChars(hSvc, nullptr, cnt, chars.data(), &cnt, BLUETOOTH_GATT_FLAG_NONE))) {
                    for (auto& ch : chars) {
                        if (ch.CharacteristicUuid.IsShortUuid &&
                            ch.CharacteristicUuid.Value.ShortUuid == 0x2A19 && ch.IsReadable) {
                            USHORT vsz = 0;
                            pGetVal(hSvc, &ch, 0, nullptr, &vsz, BLUETOOTH_GATT_FLAG_NONE);
                            if (vsz >= offsetof(BTH_LE_GATT_CHARACTERISTIC_VALUE, Data) + 1) {
                                std::vector<ULONG_PTR> valueStorage(
                                    (static_cast<size_t>(vsz) + sizeof(ULONG_PTR) - 1) / sizeof(ULONG_PTR));
                                auto* val = reinterpret_cast<PBTH_LE_GATT_CHARACTERISTIC_VALUE>(valueStorage.data());
                                if (SUCCEEDED(pGetVal(hSvc, &ch, vsz, val, nullptr, BLUETOOTH_GATT_FLAG_NONE)) &&
                                    val->DataSize >= 1 &&
                                    val->DataSize <= vsz - offsetof(BTH_LE_GATT_CHARACTERISTIC_VALUE, Data) &&
                                    val->Data[0] <= 100) {
                                    DeviceKind k = ClassifyDevice(nm, L"", 0);
                                    if (k == DeviceKind::Headphones && !headphones.online)
                                        UpdateCandidate(headphones, nm, val->Data[0], true);
                                    else if ((k == DeviceKind::Mouse || k == DeviceKind::Unknown) && !mouse.online)
                                        UpdateCandidate(mouse, nm, val->Data[0], true);
                                }
                            }
                        }
                    }
                }
            }
            CloseHandle(hSvc);
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    FreeLibrary(hBt);
}

static void ScanHidPowerFallback(DeviceState& mouse, DeviceState& headphones) {
    if (mouse.online && headphones.online) return;
    GUID hidGuid;
    HidD_GetHidGuid(&hidGuid);
    HDEVINFO set = SetupDiGetClassDevsW(&hidGuid, nullptr, nullptr, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
    if (set == INVALID_HANDLE_VALUE) return;

    SP_DEVICE_INTERFACE_DATA ifd{};
    ifd.cbSize = sizeof(ifd);
    for (DWORD i = 0; SetupDiEnumDeviceInterfaces(set, nullptr, &hidGuid, i, &ifd); ++i) {
        DWORD req = 0;
        SetupDiGetDeviceInterfaceDetailW(set, &ifd, nullptr, 0, &req, nullptr);
        if (req < offsetof(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath) + sizeof(wchar_t)) continue;
        std::vector<ULONG_PTR> storage((static_cast<size_t>(req) + sizeof(ULONG_PTR) - 1) / sizeof(ULONG_PTR));
        auto* buf = reinterpret_cast<BYTE*>(storage.data());
        auto* det = reinterpret_cast<PSP_DEVICE_INTERFACE_DETAIL_DATA_W>(buf);
        det->cbSize = sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA_W);
        if (!SetupDiGetDeviceInterfaceDetailW(set, &ifd, det, req, nullptr, nullptr)) continue;
        const size_t pathChars = (req - offsetof(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath)) / sizeof(wchar_t);
        if (std::find(det->DevicePath, det->DevicePath + pathChars, L'\0') == det->DevicePath + pathChars) continue;

        HANDLE h = CreateFileW(det->DevicePath, GENERIC_READ | GENERIC_WRITE,
                               FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0, nullptr);
        if (h == INVALID_HANDLE_VALUE) continue;

        PHIDP_PREPARSED_DATA pp = nullptr;
        if (HidD_GetPreparsedData(h, &pp)) {
            HIDP_CAPS caps{};
            if (HidP_GetCaps(pp, &caps) == HIDP_STATUS_SUCCESS &&
                (caps.UsagePage == 0x84 || caps.UsagePage == 0x85) && caps.NumberFeatureValueCaps > 0) {
                USHORT vcLen = caps.NumberFeatureValueCaps;
                std::vector<HIDP_VALUE_CAPS> vc(vcLen);
                if (HidP_GetValueCaps(HidP_Feature, vc.data(), &vcLen, pp) == HIDP_STATUS_SUCCESS) {
                    for (USHORT v = 0; v < vcLen; ++v) {
                        if (!vc[v].IsRange && vc[v].UsagePage == 0x85 &&
                           (vc[v].NotRange.Usage == 0x65 || vc[v].NotRange.Usage == 0x66)) {
                            if (caps.FeatureReportByteLength == 0) continue;
                            std::vector<BYTE> rep(caps.FeatureReportByteLength, 0);
                            rep[0] = vc[v].ReportID;
                            ULONG val = 0;
                            if (HidD_GetFeature(h, rep.data(), (ULONG)rep.size()) &&
                                HidP_GetUsageValue(HidP_Feature, 0x85, vc[v].LinkCollection,
                                    vc[v].NotRange.Usage, &val, pp, (PCHAR)rep.data(), (ULONG)rep.size()) == HIDP_STATUS_SUCCESS &&
                                val <= 100) {
                                wchar_t prod[128] = L"USB HID Device";
                                HidD_GetProductString(h, prod, sizeof(prod));
                                prod[std::size(prod) - 1] = L'\0';
                                DeviceKind k = ClassifyDevice(prod, L"", 0);
                                if (k == DeviceKind::Headphones && !headphones.online)
                                    UpdateCandidate(headphones, prod, (int)val, true);
                                else if (k == DeviceKind::Mouse && !mouse.online)
                                    UpdateCandidate(mouse, prod, (int)val, true);
                            }
                        }
                    }
                }
            }
            HidD_FreePreparsedData(pp);
        }
        CloseHandle(h);
    }
    SetupDiDestroyDeviceInfoList(set);
}

static void UpdateOneSlot(TraySlot& slot, UINT uid, wchar_t glyph, const wchar_t* label, const DeviceState& st) {
    HICON newIcon = CreateNativeIcon(glyph, st.percent, st.online);
    if (!newIcon) return;
    std::wstring tip;
    if (st.online && st.percent >= 0) {
        tip = std::wstring(label) + L": " + std::to_wstring(st.percent) + L"%";
        if (!st.name.empty()) tip += L"\n" + st.name;
    } else {
        tip = std::wstring(label) + L": не подключено";
    }
    if (tip.size() > 127) tip.resize(127);

    slot.nid.cbSize           = sizeof(NOTIFYICONDATAW);
    slot.nid.hWnd             = g_hwnd;
    slot.nid.uID              = uid;
    slot.nid.uCallbackMessage = WM_TRAY;
    slot.nid.uFlags           = NIF_ICON | NIF_TIP | NIF_MESSAGE;
    slot.nid.hIcon            = newIcon;
    wcsncpy(slot.nid.szTip, tip.c_str(), 127);
    slot.nid.szTip[127] = L'\0';

    if (Shell_NotifyIconW(slot.added ? NIM_MODIFY : NIM_ADD, &slot.nid)) {
        slot.added = true;
        if (slot.icon) DestroyIcon(slot.icon);
        slot.icon = newIcon;
    } else {
        DestroyIcon(newIcon);
    }
}

static void RefreshTrayUI() {
    DeviceState m, hp;
    {
        LockGuard lk(g_cs);
        m = g_mouse;
        hp = g_headphones;
    }
    UpdateOneSlot(g_slotMouse,      UID_MOUSE,      L'\xE962', L"Мышь",     m);
    UpdateOneSlot(g_slotHeadphones, UID_HEADPHONES, L'\xE7F6', L"Наушники", hp);
}

static void RemoveTrayAll() {
    for (TraySlot* s : {&g_slotMouse, &g_slotHeadphones}) {
        if (s->added) { Shell_NotifyIconW(NIM_DELETE, &s->nid); s->added = false; }
        if (s->icon)  { DestroyIcon(s->icon); s->icon = nullptr; }
    }
}

static const wchar_t* RUN_KEY = L"Software\\Microsoft\\Windows\\CurrentVersion\\Run";
static const wchar_t* RUN_VAL = L"TrayBatteryCpp";

static bool IsAutostart() {
    HKEY k = nullptr;
    if (RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_QUERY_VALUE, &k) != ERROR_SUCCESS) return false;
    DWORD cb = 0;
    bool ok = RegQueryValueExW(k, RUN_VAL, nullptr, nullptr, nullptr, &cb) == ERROR_SUCCESS;
    RegCloseKey(k);
    return ok;
}

static void ToggleAutostart() {
    HKEY k = nullptr;
    if (RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_SET_VALUE, &k) != ERROR_SUCCESS) return;
    if (IsAutostart()) {
        RegDeleteValueW(k, RUN_VAL);
    } else {
        wchar_t path[MAX_PATH]{};
        GetModuleFileNameW(nullptr, path, MAX_PATH);
        std::wstring val = L"\"" + std::wstring(path) + L"\"";
        RegSetValueExW(k, RUN_VAL, 0, REG_SZ, (const BYTE*)val.c_str(), (DWORD)((val.size() + 1) * sizeof(wchar_t)));
    }
    RegCloseKey(k);
}

static DWORD WINAPI WorkerThread(LPVOID) {
    HANDLE handles[2] = {g_stopEv, g_refreshEv};
    for (;;) {
        DeviceState m, hp;
        ScanPnPBluetooth(m, hp);
        ScanBleGattFallback(m, hp);
        ScanHidPowerFallback(m, hp);
        {
            LockGuard lk(g_cs);
            g_mouse = std::move(m);
            g_headphones = std::move(hp);
        }
        PostMessageW(g_hwnd, WM_UPDATE_UI, 0, 0);

        if (WaitForMultipleObjects(2, handles, FALSE, POLL_INTERVAL_MS) == WAIT_OBJECT_0)
            break;
    }
    return 0;
}

static LRESULT CALLBACK WndProc(HWND h, UINT msg, WPARAM w, LPARAM l) {
    if (g_taskbarCreated && msg == g_taskbarCreated) {
        g_slotMouse.added = false;
        g_slotHeadphones.added = false;
        RefreshTrayUI();
        return 0;
    }
    switch (msg) {
        case WM_UPDATE_UI:
        case WM_SETTINGCHANGE:
            RefreshTrayUI();
            return 0;

        case WM_DEVICECHANGE:
            if (w == DBT_DEVNODES_CHANGED)
                SetTimer(h, TIMER_DEBOUNCE, 2000, nullptr);
            return 0;

        case WM_TIMER:
            if (w == TIMER_DEBOUNCE) {
                KillTimer(h, TIMER_DEBOUNCE);
                SetEvent(g_refreshEv);
            }
            return 0;

        case WM_TRAY:
            if (LOWORD(l) == WM_RBUTTONUP || LOWORD(l) == WM_CONTEXTMENU) {
                HMENU m = CreatePopupMenu();
                AppendMenuW(m, MF_STRING, 1, L"Обновить сейчас");
                AppendMenuW(m, MF_STRING | (IsAutostart() ? MF_CHECKED : MF_UNCHECKED), 2, L"Запускать с Windows");
                AppendMenuW(m, MF_SEPARATOR, 0, nullptr);
                AppendMenuW(m, MF_STRING, 3, L"Выход");
                POINT pt; GetCursorPos(&pt);
                SetForegroundWindow(h);
                TrackPopupMenu(m, TPM_RIGHTBUTTON, pt.x, pt.y, 0, h, nullptr);
                DestroyMenu(m);
            }
            return 0;

        case WM_COMMAND:
            if (LOWORD(w) == 1) SetEvent(g_refreshEv);
            else if (LOWORD(w) == 2) ToggleAutostart();
            else if (LOWORD(w) == 3) DestroyWindow(h);
            return 0;

        case WM_DESTROY:
            RemoveTrayAll();
            PostQuitMessage(0);
            return 0;
    }
    return DefWindowProcW(h, msg, w, l);
}

int WINAPI wWinMain(HINSTANCE hInst, HINSTANCE, PWSTR, int) {
    HANDLE mtx = CreateMutexW(nullptr, TRUE, L"TrayBatteryCpp_SingleInstance");
    if (!mtx) return 1;
    if (GetLastError() == ERROR_ALREADY_EXISTS) {
        CloseHandle(mtx);
        return 0;
    }

    using SetDpiCtxFn = BOOL(WINAPI*)(HANDLE);
    if (auto fn = (SetDpiCtxFn)GetProcAddress(GetModuleHandleW(L"user32.dll"), "SetProcessDpiAwarenessContext"))
        fn((HANDLE)-4);

    InitializeCriticalSection(&g_cs);
    g_taskbarCreated = RegisterWindowMessageW(L"TaskbarCreated");
    g_stopEv    = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    g_refreshEv = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!g_stopEv || !g_refreshEv) {
        if (g_stopEv) CloseHandle(g_stopEv);
        if (g_refreshEv) CloseHandle(g_refreshEv);
        DeleteCriticalSection(&g_cs);
        ReleaseMutex(mtx);
        CloseHandle(mtx);
        return 1;
    }

    WNDCLASSW wc{};
    wc.lpfnWndProc   = WndProc;
    wc.hInstance     = hInst;
    wc.lpszClassName = L"TrayBatteryCppWnd";
    if (!RegisterClassW(&wc)) {
        CloseHandle(g_stopEv);
        CloseHandle(g_refreshEv);
        DeleteCriticalSection(&g_cs);
        ReleaseMutex(mtx);
        CloseHandle(mtx);
        return 1;
    }

    g_hwnd = CreateWindowExW(0, wc.lpszClassName, L"TrayBattery", WS_OVERLAPPEDWINDOW,
                             0, 0, 0, 0, nullptr, nullptr, hInst, nullptr);
    if (!g_hwnd) {
        CloseHandle(g_stopEv);
        CloseHandle(g_refreshEv);
        DeleteCriticalSection(&g_cs);
        ReleaseMutex(mtx);
        CloseHandle(mtx);
        return 1;
    }
    RefreshTrayUI();

    HANDLE th = CreateThread(nullptr, 0, WorkerThread, nullptr, 0, nullptr);
    if (!th) {
        DestroyWindow(g_hwnd);
        CloseHandle(g_stopEv);
        CloseHandle(g_refreshEv);
        DeleteCriticalSection(&g_cs);
        ReleaseMutex(mtx);
        CloseHandle(mtx);
        return 1;
    }
    MSG msg;
    while (GetMessageW(&msg, nullptr, 0, 0) > 0) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    SetEvent(g_stopEv);
    WaitForSingleObject(th, INFINITE);
    CloseHandle(th);
    RemoveTrayAll();
    CloseHandle(g_stopEv);
    CloseHandle(g_refreshEv);
    DeleteCriticalSection(&g_cs);
    ReleaseMutex(mtx);
    CloseHandle(mtx);
    return 0;
}