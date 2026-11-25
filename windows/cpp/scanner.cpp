#include "scanner.hpp"

#include <bluetoothleapis.h>
#include <devpropdef.h>
#include <mmdeviceapi.h>
#include <objbase.h>
#include <propsys.h>
#include <setupapi.h>

extern "C" {
#include <hidsdi.h>
#include <hidpi.h>
}

#include <algorithm>
#include <cstddef>
#include <cstdlib>
#include <cstring>
#include <cwctype>
#include <unordered_map>
#include <vector>

namespace {

constexpr DEVPROPKEY kPropBluetoothBattery = {
    {0x104EA319, 0x6EE2, 0x4701, {0xBD, 0x47, 0x8D, 0xDB, 0xF4, 0x25, 0xBB, 0xE5}}, 2
};
constexpr DEVPROPKEY kPropBluetoothAddress = {
    {0x2BD67D8B, 0x8BEB, 0x48D5, {0x87, 0xE0, 0x6C, 0xDA, 0x34, 0x28, 0x04, 0x0A}}, 1
};
constexpr DEVPROPKEY kPropBluetoothCoD = {
    {0x2BD67D8B, 0x8BEB, 0x48D5, {0x87, 0xE0, 0x6C, 0xDA, 0x34, 0x28, 0x04, 0x0A}}, 10
};
constexpr DEVPROPKEY kPropFriendlyName = {
    {0xA45C254E, 0xDF1C, 0x4EFD, {0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0}}, 14
};
constexpr DEVPROPKEY kPropDeviceDesc = {
    {0xA45C254E, 0xDF1C, 0x4EFD, {0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0}}, 2
};
constexpr DEVPROPKEY kPropDeviceClass = {
    {0xA45C254E, 0xDF1C, 0x4EFD, {0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0}}, 9
};
constexpr DEVPROPKEY kPropContainerId = {
    {0x8C7ED206, 0x3F8A, 0x4827, {0xB3, 0xAB, 0xAE, 0x9E, 0x1F, 0xAE, 0xFC, 0x6C}}, 2
};
constexpr DEVPROPKEY kPropIsConnected = {
    {0x83DA6326, 0x97A6, 0x4088, {0x94, 0x53, 0xA1, 0x92, 0x3F, 0x57, 0x3B, 0x29}}, 15
};

constexpr PROPERTYKEY kPkeyAudioContainerId = {
    {0x8C7ED206, 0x3F8A, 0x4827, {0xB3, 0xAB, 0xAE, 0x9E, 0x1F, 0xAE, 0xFC, 0x6C}}, 2
};
constexpr PROPERTYKEY kPkeyAudioFriendlyName = {
    {0xA45C254E, 0xDF1C, 0x4EFD, {0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0}}, 14
};

constexpr GUID kClsidMMDeviceEnumerator = {
    0xBCDE0395, 0xE52F, 0x467C, {0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E}
};
constexpr GUID kIidIMMDeviceEnumerator = {
    0xA95664D2, 0x9614, 0x4F35, {0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6}
};
constexpr GUID kGuidBthLeBatteryService = {
    0x0000180F, 0x0000, 0x1000, {0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB}
};

struct DevInfoGuard {
    HDEVINFO set = INVALID_HANDLE_VALUE;
    explicit DevInfoGuard(HDEVINFO s) : set(s) {}
    ~DevInfoGuard() { if (set != INVALID_HANDLE_VALUE) SetupDiDestroyDeviceInfoList(set); }
    DevInfoGuard(const DevInfoGuard&) = delete;
    DevInfoGuard& operator=(const DevInfoGuard&) = delete;
};

struct HandleGuard {
    HANDLE h = INVALID_HANDLE_VALUE;
    explicit HandleGuard(HANDLE handle) : h(handle) {}
    ~HandleGuard() { if (h != INVALID_HANDLE_VALUE && h != nullptr) CloseHandle(h); }
    HandleGuard(const HandleGuard&) = delete;
    HandleGuard& operator=(const HandleGuard&) = delete;
};

struct ModuleGuard {
    HMODULE mod = nullptr;
    explicit ModuleGuard(HMODULE m) : mod(m) {}
    ~ModuleGuard() { if (mod) FreeLibrary(mod); }
    ModuleGuard(const ModuleGuard&) = delete;
    ModuleGuard& operator=(const ModuleGuard&) = delete;
};

struct HidPreparsedGuard {
    PHIDP_PREPARSED_DATA ppd = nullptr;
    ~HidPreparsedGuard() { if (ppd) HidD_FreePreparsedData(ppd); }
    HidPreparsedGuard() = default;
    HidPreparsedGuard(const HidPreparsedGuard&) = delete;
    HidPreparsedGuard& operator=(const HidPreparsedGuard&) = delete;
};

struct ActiveAudioInfo {
    std::wstring containerId;
    std::wstring friendlyName;
};

bool IsZeroGuid(const GUID& g) {
    static constexpr GUID kZero{};
    return std::memcmp(&g, &kZero, sizeof(GUID)) == 0;
}

std::wstring FormatGuidLower(const GUID& g) {
    if (IsZeroGuid(g)) return {};
    wchar_t buf[64]{};
    if (StringFromGUID2(g, buf, _countof(buf)) <= 0) return {};
    return ToLower(buf);
}

uint64_t ParseBluetoothAddress(const std::wstring& raw) {
    wchar_t hex[13]{};
    size_t count = 0;
    for (wchar_t ch : raw) {
        if (ch == L':' || ch == L'-' || ch == L' ') continue;
        if (!std::iswxdigit(ch) || count >= 12) return 0;
        hex[count++] = ch;
    }
    if (count != 12) return 0;
    return std::wcstoull(hex, nullptr, 16);
}

uint64_t ExtractBthAddrFromInstanceId(HDEVINFO set, SP_DEVINFO_DATA& dev) {
    wchar_t inst[256]{};
    if (!SetupDiGetDeviceInstanceIdW(set, &dev, inst, _countof(inst), nullptr)) return 0;
    const std::wstring up = ToLower(inst);
    for (const wchar_t* tag : {L"dev_", L"bluetoothdevice_"}) {
        size_t pos = up.find(tag);
        if (pos == std::wstring::npos) continue;
        pos += std::wcslen(tag);
        if (pos + 12 <= up.size()) {
            const uint64_t addr = ParseBluetoothAddress(up.substr(pos, 12));
            if (addr != 0) return addr;
        }
    }
    return 0;
}

ActiveAudioInfo QueryActiveAudioEndpoint() {
    ActiveAudioInfo info{};
    IMMDeviceEnumerator* enumerator = nullptr;
    if (FAILED(CoCreateInstance(kClsidMMDeviceEnumerator, nullptr, CLSCTX_INPROC_SERVER,
            kIidIMMDeviceEnumerator, reinterpret_cast<void**>(&enumerator))) || !enumerator) {
        return info;
    }

    IMMDevice* dev = nullptr;
    HRESULT hr = enumerator->GetDefaultAudioEndpoint(eRender, eConsole, &dev);
    if (FAILED(hr) || !dev) {
        hr = enumerator->GetDefaultAudioEndpoint(eRender, eMultimedia, &dev);
    }

    if (SUCCEEDED(hr) && dev) {
        IPropertyStore* store = nullptr;
        if (SUCCEEDED(dev->OpenPropertyStore(STGM_READ, &store)) && store) {
            PROPVARIANT pv;
            PropVariantInit(&pv);
            if (SUCCEEDED(store->GetValue(kPkeyAudioContainerId, &pv))) {
                if (pv.vt == VT_LPWSTR && pv.pwszVal) {
                    std::wstring cid = ToLower(pv.pwszVal);
                    if (cid != L"{00000000-0000-0000-0000-000000000000}") info.containerId = std::move(cid);
                } else if (pv.vt == VT_CLSID && pv.puuid) {
                    info.containerId = FormatGuidLower(*pv.puuid);
                }
            }
            PropVariantClear(&pv);

            PropVariantInit(&pv);
            if (SUCCEEDED(store->GetValue(kPkeyAudioFriendlyName, &pv))) {
                if (pv.vt == VT_LPWSTR && pv.pwszVal) info.friendlyName = pv.pwszVal;
            }
            PropVariantClear(&pv);
            store->Release();
        }
        dev->Release();
    }

    enumerator->Release();
    return info;
}

bool MatchesActiveAudio(const ActiveAudioInfo& audio, const std::wstring& cid, const std::wstring& name) {
    if (!audio.containerId.empty() && !cid.empty() && audio.containerId == cid) return true;
    return NamesMatchAudio(name, audio.friendlyName);
}

std::wstring ReadDevPropString(HDEVINFO set, SP_DEVINFO_DATA& dev, const DEVPROPKEY& key) {
    DEVPROPTYPE type = 0;
    DWORD reqBytes = 0;
    SetupDiGetDevicePropertyW(set, &dev, &key, &type, nullptr, 0, &reqBytes, 0);
    if (reqBytes < sizeof(wchar_t) || type != DEVPROP_TYPE_STRING) return {};

    std::vector<wchar_t> buf(reqBytes / sizeof(wchar_t) + 1, L'\0');
    if (!SetupDiGetDevicePropertyW(set, &dev, &key, &type,
            reinterpret_cast<PBYTE>(buf.data()), reqBytes, &reqBytes, 0)) {
        return {};
    }
    buf.back() = L'\0';
    return std::wstring(buf.data());
}

bool ReadDevPropByte(HDEVINFO set, SP_DEVINFO_DATA& dev, const DEVPROPKEY& key, uint8_t& out) {
    DEVPROPTYPE type = 0;
    BYTE val = 0;
    DWORD req = 0;
    if (!SetupDiGetDevicePropertyW(set, &dev, &key, &type, &val, sizeof(val), &req, 0)) return false;
    if (type != DEVPROP_TYPE_BYTE || req < 1) return false;
    out = val;
    return true;
}

int ReadDevPropBool(HDEVINFO set, SP_DEVINFO_DATA& dev, const DEVPROPKEY& key) {
    DEVPROPTYPE type = 0;
    DEVPROP_BOOLEAN val = 0;
    DWORD req = 0;
    if (!SetupDiGetDevicePropertyW(set, &dev, &key, &type,
            reinterpret_cast<PBYTE>(&val), sizeof(val), &req, 0)) {
        return 0;
    }
    if (type != DEVPROP_TYPE_BOOLEAN || req < 1) return 0;
    return (val != 0) ? 1 : -1;
}

std::wstring ReadDevPropGuid(HDEVINFO set, SP_DEVINFO_DATA& dev, const DEVPROPKEY& key) {
    DEVPROPTYPE type = 0;
    GUID g{};
    DWORD req = 0;
    if (!SetupDiGetDevicePropertyW(set, &dev, &key, &type,
            reinterpret_cast<PBYTE>(&g), sizeof(g), &req, 0)) {
        return {};
    }
    if (type != DEVPROP_TYPE_GUID || req < sizeof(GUID)) return {};
    return FormatGuidLower(g);
}

uint32_t ReadDevPropU32(HDEVINFO set, SP_DEVINFO_DATA& dev, const DEVPROPKEY& key) {
    DEVPROPTYPE type = 0;
    uint32_t val = 0;
    DWORD req = 0;
    if (!SetupDiGetDevicePropertyW(set, &dev, &key, &type,
            reinterpret_cast<PBYTE>(&val), sizeof(val), &req, 0)) {
        return 0;
    }
    return (type == DEVPROP_TYPE_UINT32 && req >= sizeof(val)) ? val : 0;
}

uint64_t ResolveBluetoothAddress(HDEVINFO set, SP_DEVINFO_DATA& dev) {
    uint64_t addr = ParseBluetoothAddress(ReadDevPropString(set, dev, kPropBluetoothAddress));
    if (addr == 0) addr = ExtractBthAddrFromInstanceId(set, dev);
    return addr;
}

std::wstring ResolveDevName(HDEVINFO set, SP_DEVINFO_DATA& dev) {
    std::wstring name = ReadDevPropString(set, dev, kPropFriendlyName);
    if (name.empty()) name = ReadDevPropString(set, dev, kPropDeviceDesc);
    return name;
}

void ScanPnpBluetooth(std::array<DeviceState, 3>& out, const ActiveAudioInfo& audio) {
    DevInfoGuard set(SetupDiGetClassDevsW(nullptr, nullptr, nullptr, DIGCF_ALLCLASSES | DIGCF_PRESENT));
    if (set.set == INVALID_HANDLE_VALUE) return;

    struct RawCandidate {
        std::wstring name;
        std::wstring pnpClass;
        std::wstring containerId;
        uint64_t bthAddr = 0;
        uint32_t cod = 0;
        int pct = -1;
        int nodeConn = 0;
    };

    std::unordered_map<std::wstring, int> containerConn;
    std::unordered_map<std::wstring, uint64_t> containerAddr;
    std::vector<RawCandidate> raw;

    SP_DEVINFO_DATA dev{};
    dev.cbSize = sizeof(dev);
    for (DWORD i = 0; SetupDiEnumDeviceInfo(set.set, i, &dev); ++i) {
        const std::wstring cid = ReadDevPropGuid(set.set, dev, kPropContainerId);
        const int connState = ReadDevPropBool(set.set, dev, kPropIsConnected);
        const uint64_t nodeAddr = ResolveBluetoothAddress(set.set, dev);
        if (!cid.empty()) {
            if (connState != 0) {
                auto& slot = containerConn[cid];
                if (connState > 0) slot = 1;
                else if (slot == 0) slot = -1;
            }
            if (nodeAddr != 0 && containerAddr[cid] == 0) {
                containerAddr[cid] = nodeAddr;
            }
        }

        uint8_t pct = 0;
        if (!ReadDevPropByte(set.set, dev, kPropBluetoothBattery, pct) || pct > 100) continue;

        RawCandidate cand{};
        cand.name = ResolveDevName(set.set, dev);
        cand.pnpClass = ReadDevPropString(set.set, dev, kPropDeviceClass);
        cand.containerId = cid;
        cand.bthAddr = nodeAddr;
        cand.cod = ReadDevPropU32(set.set, dev, kPropBluetoothCoD);
        cand.pct = pct;
        cand.nodeConn = connState;
        raw.push_back(std::move(cand));
    }

    for (const auto& cand : raw) {
        int state = cand.nodeConn;
        uint64_t addr = cand.bthAddr;
        if (!cand.containerId.empty()) {
            const auto it = containerConn.find(cand.containerId);
            if (it != containerConn.end()) state = it->second;
            if (addr == 0) {
                const auto ait = containerAddr.find(cand.containerId);
                if (ait != containerAddr.end()) addr = ait->second;
            }
        }
        // Skip paired Bluetooth devices explicitly reported as disconnected (stale cached battery)
        if (state < 0) continue;

        const bool isActive = MatchesActiveAudio(audio, cand.containerId, cand.name);
        DeviceKind kind = ClassifyDevice(cand.name, cand.pnpClass, cand.cod);
        if (kind == DeviceKind::Unknown && isActive) kind = DeviceKind::Headphones;

        DeviceEntry entry{};
        entry.name = cand.name;
        entry.containerId = cand.containerId;
        entry.bthAddr = addr;
        entry.percent = cand.pct;
        entry.activeAudio = isActive;
        entry.connected = (state > 0);
        StoreCandidate(out, kind, entry);
    }
}

using FnGetChars = HRESULT (WINAPI*)(HANDLE, PBTH_LE_GATT_SERVICE, USHORT, PBTH_LE_GATT_CHARACTERISTIC, USHORT*, ULONG);
using FnGetCharVal = HRESULT (WINAPI*)(HANDLE, PBTH_LE_GATT_CHARACTERISTIC, ULONG, PBTH_LE_GATT_CHARACTERISTIC_VALUE, USHORT*, ULONG);

void ScanBleGatt(std::array<DeviceState, 3>& out, const ActiveAudioInfo& audio) {
    ModuleGuard dll(LoadLibraryExW(L"BluetoothAPIs.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32));
    if (!dll.mod) return;

    auto getChars = reinterpret_cast<FnGetChars>(GetProcAddress(dll.mod, "BluetoothGATTGetCharacteristics"));
    auto getCharVal = reinterpret_cast<FnGetCharVal>(GetProcAddress(dll.mod, "BluetoothGATTGetCharacteristicValue"));
    if (!getChars || !getCharVal) return;

    GUID svcGuid = kGuidBthLeBatteryService;
    DevInfoGuard set(SetupDiGetClassDevsW(&svcGuid, nullptr, nullptr, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE));
    if (set.set == INVALID_HANDLE_VALUE) return;

    SP_DEVICE_INTERFACE_DATA ifData{};
    ifData.cbSize = sizeof(ifData);
    for (DWORD i = 0; SetupDiEnumDeviceInterfaces(set.set, nullptr, &svcGuid, i, &ifData); ++i) {
        DWORD reqSize = 0;
        SetupDiGetDeviceInterfaceDetailW(set.set, &ifData, nullptr, 0, &reqSize, nullptr);
        if (reqSize < offsetof(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath) + sizeof(wchar_t)) continue;

        std::vector<uint64_t> detailRaw((reqSize + 7) / 8, 0);
        auto* detail = reinterpret_cast<SP_DEVICE_INTERFACE_DETAIL_DATA_W*>(detailRaw.data());
        detail->cbSize = sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA_W);

        SP_DEVINFO_DATA dev{};
        dev.cbSize = sizeof(dev);
        if (!SetupDiGetDeviceInterfaceDetailW(set.set, &ifData, detail, reqSize, nullptr, &dev)) continue;

        if (ReadDevPropBool(set.set, dev, kPropIsConnected) < 0) continue;

        HandleGuard h(CreateFileW(detail->DevicePath, GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr));
        if (h.h == INVALID_HANDLE_VALUE) continue;

        USHORT charCount = 0;
        getChars(h.h, nullptr, 0, nullptr, &charCount, BLUETOOTH_GATT_FLAG_NONE);
        if (charCount == 0) continue;

        std::vector<BTH_LE_GATT_CHARACTERISTIC> chars(charCount);
        USHORT actualCount = 0;
        if (FAILED(getChars(h.h, nullptr, charCount, chars.data(), &actualCount, BLUETOOTH_GATT_FLAG_NONE))) {
            continue;
        }

        const size_t limit = std::min<size_t>(actualCount, chars.size());
        for (size_t c = 0; c < limit; ++c) {
            if (!chars[c].CharacteristicUuid.IsShortUuid || chars[c].CharacteristicUuid.Value.ShortUuid != 0x2A19) {
                continue;
            }
            USHORT valSize = 0;
            getCharVal(h.h, &chars[c], 0, nullptr, &valSize, BLUETOOTH_GATT_FLAG_NONE);
            if (valSize < offsetof(BTH_LE_GATT_CHARACTERISTIC_VALUE, Data) + 1) continue;

            std::vector<uint64_t> valRaw((valSize + 7) / 8, 0);
            auto* val = reinterpret_cast<PBTH_LE_GATT_CHARACTERISTIC_VALUE>(valRaw.data());
            if (SUCCEEDED(getCharVal(h.h, &chars[c], valSize, val, nullptr, BLUETOOTH_GATT_FLAG_NONE))) {
                const size_t maxPayload = valSize - offsetof(BTH_LE_GATT_CHARACTERISTIC_VALUE, Data);
                if (val->DataSize >= 1 && val->DataSize <= maxPayload && val->Data[0] <= 100) {
                    const std::wstring name = ResolveDevName(set.set, dev);
                    const std::wstring cls = ReadDevPropString(set.set, dev, kPropDeviceClass);
                    const std::wstring cid = ReadDevPropGuid(set.set, dev, kPropContainerId);
                    const uint64_t addr = ResolveBluetoothAddress(set.set, dev);
                    const uint32_t cod = ReadDevPropU32(set.set, dev, kPropBluetoothCoD);
                    const bool isActive = MatchesActiveAudio(audio, cid, name);
                    DeviceKind kind = ClassifyDevice(name, cls, cod);
                    if (kind == DeviceKind::Unknown && isActive) kind = DeviceKind::Headphones;

                    DeviceEntry entry{};
                    entry.name = name;
                    entry.containerId = cid;
                    entry.bthAddr = addr;
                    entry.percent = val->Data[0];
                    entry.activeAudio = isActive;
                    entry.connected = true;
                    StoreCandidate(out, kind, entry);
                    break;
                }
            }
        }
    }
}

void ScanUsbHidPower(std::array<DeviceState, 3>& out, const ActiveAudioInfo& audio) {
    GUID hidGuid{};
    HidD_GetHidGuid(&hidGuid);
    DevInfoGuard set(SetupDiGetClassDevsW(&hidGuid, nullptr, nullptr, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE));
    if (set.set == INVALID_HANDLE_VALUE) return;

    SP_DEVICE_INTERFACE_DATA ifData{};
    ifData.cbSize = sizeof(ifData);
    for (DWORD i = 0; SetupDiEnumDeviceInterfaces(set.set, nullptr, &hidGuid, i, &ifData); ++i) {
        DWORD reqSize = 0;
        SetupDiGetDeviceInterfaceDetailW(set.set, &ifData, nullptr, 0, &reqSize, nullptr);
        if (reqSize < offsetof(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath) + sizeof(wchar_t)) continue;

        std::vector<uint64_t> detailRaw((reqSize + 7) / 8, 0);
        auto* detail = reinterpret_cast<SP_DEVICE_INTERFACE_DETAIL_DATA_W*>(detailRaw.data());
        detail->cbSize = sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA_W);

        SP_DEVINFO_DATA dev{};
        dev.cbSize = sizeof(dev);
        if (!SetupDiGetDeviceInterfaceDetailW(set.set, &ifData, detail, reqSize, nullptr, &dev)) continue;

        HandleGuard h(CreateFileW(detail->DevicePath, 0,
            FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0, nullptr));
        if (h.h == INVALID_HANDLE_VALUE) continue;

        HidPreparsedGuard ppd;
        if (!HidD_GetPreparsedData(h.h, &ppd.ppd) || !ppd.ppd) continue;

        HIDP_CAPS caps{};
        if (HidP_GetCaps(ppd.ppd, &caps) != HIDP_STATUS_SUCCESS) continue;
        if (caps.FeatureReportByteLength < 2 || caps.NumberFeatureValueCaps == 0) continue;

        USHORT capCount = caps.NumberFeatureValueCaps;
        std::vector<HIDP_VALUE_CAPS> valueCaps(capCount);
        if (HidP_GetValueCaps(HidP_Feature, valueCaps.data(), &capCount, ppd.ppd) != HIDP_STATUS_SUCCESS) {
            continue;
        }

        for (USHORT c = 0; c < capCount; ++c) {
            const auto& vc = valueCaps[c];
            if (vc.IsRange) continue;
            const bool isPowerPage = (vc.UsagePage == 0x84 || vc.UsagePage == 0x85);
            const bool isBatUsage = (vc.NotRange.Usage == 0x66 || vc.NotRange.Usage == 0x65);
            if (!isPowerPage || !isBatUsage) continue;

            std::vector<uint8_t> report(caps.FeatureReportByteLength, 0);
            report[0] = vc.ReportID;
            if (!HidD_GetFeature(h.h, report.data(), static_cast<ULONG>(report.size()))) continue;

            ULONG rawVal = 0;
            if (HidP_GetUsageValue(HidP_Feature, vc.UsagePage, vc.LinkCollection, vc.NotRange.Usage,
                    &rawVal, ppd.ppd, reinterpret_cast<PCHAR>(report.data()),
                    static_cast<ULONG>(report.size())) != HIDP_STATUS_SUCCESS) {
                continue;
            }

            int pct = -1;
            if (vc.LogicalMax > vc.LogicalMin && vc.LogicalMax > 0) {
                const double norm = static_cast<double>(static_cast<long>(rawVal) - vc.LogicalMin) /
                                    static_cast<double>(vc.LogicalMax - vc.LogicalMin);
                pct = std::clamp(static_cast<int>(norm * 100.0 + 0.5), 0, 100);
            } else if (rawVal <= 100) {
                pct = static_cast<int>(rawVal);
            }
            if (pct < 0) continue;

            wchar_t prod[128]{};
            std::wstring name;
            if (HidD_GetProductString(h.h, prod, sizeof(prod) - sizeof(wchar_t))) {
                prod[127] = L'\0';
                name = prod;
            }
            if (name.empty()) name = ResolveDevName(set.set, dev);
            const std::wstring cls = ReadDevPropString(set.set, dev, kPropDeviceClass);
            const std::wstring cid = ReadDevPropGuid(set.set, dev, kPropContainerId);
            const bool isActive = MatchesActiveAudio(audio, cid, name);
            DeviceKind kind = ClassifyDevice(name, cls);
            if (kind == DeviceKind::Unknown && isActive) kind = DeviceKind::Headphones;

            DeviceEntry entry{};
            entry.name = name;
            entry.containerId = cid;
            entry.percent = pct;
            entry.activeAudio = isActive;
            entry.connected = true;
            StoreCandidate(out, kind, entry);
            break;
        }
    }
}

struct BluetoothFindRadioParams {
    DWORD dwSize;
};

using FnFindFirstRadio = HANDLE (WINAPI*)(const BluetoothFindRadioParams*, HANDLE*);
using FnFindNextRadio = BOOL (WINAPI*)(HANDLE, HANDLE*);
using FnFindRadioClose = BOOL (WINAPI*)(HANDLE);

} // namespace

std::array<DeviceState, 3> ScanAllDevices() {
    std::array<DeviceState, 3> out{};
    const ActiveAudioInfo audio = QueryActiveAudioEndpoint();
    ScanPnpBluetooth(out, audio);
    ScanBleGatt(out, audio);
    ScanUsbHidPower(out, audio);
    FinalizeDevices(out);
    return out;
}

bool DisconnectBluetoothDevice(uint64_t bthAddr) {
    if (bthAddr == 0) return false;
    ModuleGuard dll(LoadLibraryExW(L"BluetoothAPIs.dll", nullptr, LOAD_LIBRARY_SEARCH_SYSTEM32));
    if (!dll.mod) return false;

    auto findFirst = reinterpret_cast<FnFindFirstRadio>(GetProcAddress(dll.mod, "BluetoothFindFirstRadio"));
    auto findNext = reinterpret_cast<FnFindNextRadio>(GetProcAddress(dll.mod, "BluetoothFindNextRadio"));
    auto findClose = reinterpret_cast<FnFindRadioClose>(GetProcAddress(dll.mod, "BluetoothFindRadioClose"));
    if (!findFirst || !findNext || !findClose) return false;

    BluetoothFindRadioParams params{sizeof(BluetoothFindRadioParams)};
    HANDLE hRadio = nullptr;
    HANDLE hFind = findFirst(&params, &hRadio);
    if (!hFind) return false;

    // IOCTL_BTH_DISCONNECT_DEVICE (FILE_DEVICE_BLUETOOTH=0x41, function=0x03, METHOD_BUFFERED)
    constexpr DWORD kIoctlBthDisconnectDevice = 0x0041000C;
    bool disconnected = false;
    do {
        if (hRadio) {
            DWORD bytes = 0;
            if (DeviceIoControl(hRadio, kIoctlBthDisconnectDevice, &bthAddr, sizeof(bthAddr),
                                nullptr, 0, &bytes, nullptr)) {
                disconnected = true;
            }
            CloseHandle(hRadio);
            hRadio = nullptr;
        }
    } while (findNext(hFind, &hRadio));

    findClose(hFind);
    return disconnected;
}