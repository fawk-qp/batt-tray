#include "scanner.hpp"

#include <bluetoothleapis.h>
#include <devpropdef.h>
#include <setupapi.h>

extern "C" {
#include <hidsdi.h>
#include <hidpi.h>
}

#include <algorithm>
#include <cstddef>
#include <cstring>
#include <vector>

namespace {

constexpr DEVPROPKEY kPropBluetoothBattery = {
    {0x104EA319, 0x6EE2, 0x4701, {0xBD, 0x47, 0x8D, 0xDB, 0xF4, 0x25, 0xBB, 0xE5}}, 2
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

// Standard Bluetooth LE GATT Battery Service UUID (0x180F)
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

bool AllOnline(const std::array<DeviceState, 3>& out) {
    return out[0].online && out[1].online && out[2].online;
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

std::wstring ResolveDevName(HDEVINFO set, SP_DEVINFO_DATA& dev) {
    std::wstring name = ReadDevPropString(set, dev, kPropFriendlyName);
    if (name.empty()) name = ReadDevPropString(set, dev, kPropDeviceDesc);
    return name;
}

void ScanPnpBluetooth(std::array<DeviceState, 3>& out) {
    DevInfoGuard set(SetupDiGetClassDevsW(nullptr, nullptr, nullptr, DIGCF_ALLCLASSES | DIGCF_PRESENT));
    if (set.set == INVALID_HANDLE_VALUE) return;

    SP_DEVINFO_DATA dev{};
    dev.cbSize = sizeof(dev);
    for (DWORD i = 0; SetupDiEnumDeviceInfo(set.set, i, &dev); ++i) {
        uint8_t pct = 0;
        if (!ReadDevPropByte(set.set, dev, kPropBluetoothBattery, pct) || pct > 100) continue;

        const std::wstring name = ResolveDevName(set.set, dev);
        const std::wstring cls = ReadDevPropString(set.set, dev, kPropDeviceClass);
        const uint32_t cod = ReadDevPropU32(set.set, dev, kPropBluetoothCoD);
        StoreCandidate(out, ClassifyDevice(name, cls, cod), name, pct);
    }
}

using FnGetChars = HRESULT (WINAPI*)(HANDLE, PBTH_LE_GATT_SERVICE, USHORT, PBTH_LE_GATT_CHARACTERISTIC, USHORT*, ULONG);
using FnGetCharVal = HRESULT (WINAPI*)(HANDLE, PBTH_LE_GATT_CHARACTERISTIC, ULONG, PBTH_LE_GATT_CHARACTERISTIC_VALUE, USHORT*, ULONG);

void ScanBleGatt(std::array<DeviceState, 3>& out) {
    if (AllOnline(out)) return;

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
                    const uint32_t cod = ReadDevPropU32(set.set, dev, kPropBluetoothCoD);
                    StoreCandidate(out, ClassifyDevice(name, cls, cod), name, val->Data[0]);
                    break;
                }
            }
        }
    }
}

void ScanUsbHidPower(std::array<DeviceState, 3>& out) {
    if (AllOnline(out)) return;

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

        // Access mode 0 allows querying feature reports even on mice/keyboards locked by Windows
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
            // HID Power Device (0x84) / Battery System (0x85): RemainingCapacity (0x66) or AbsoluteStateOfCharge (0x65)
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
            StoreCandidate(out, ClassifyDevice(name, cls), name, pct);
            break;
        }
    }
}

} // namespace

std::array<DeviceState, 3> ScanAllDevices() {
    std::array<DeviceState, 3> out{};
    ScanPnpBluetooth(out);
    ScanBleGatt(out);
    ScanUsbHidPower(out);
    return out;
}