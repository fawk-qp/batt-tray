#include "vendor_hid.hpp"

#include <setupapi.h>

extern "C" {
#include <hidsdi.h>
#include <hidpi.h>
}

#include <algorithm>
#include <cstddef>
#include <cstring>
#include <mutex>
#include <unordered_map>
#include <vector>

namespace {

constexpr USHORT kVidLogitech = 0x046D;
constexpr USHORT kVidRazer = 0x1532;
constexpr uint8_t kHidppSwId = 0x0B;

struct HandleGuard {
    HANDLE h = INVALID_HANDLE_VALUE;
    explicit HandleGuard(HANDLE handle) : h(handle) {}
    ~HandleGuard() {
        if (h != INVALID_HANDLE_VALUE && h != nullptr) CloseHandle(h);
    }
    HandleGuard(const HandleGuard&) = delete;
    HandleGuard& operator=(const HandleGuard&) = delete;
};

struct DevInfoGuard {
    HDEVINFO set = INVALID_HANDLE_VALUE;
    explicit DevInfoGuard(HDEVINFO s) : set(s) {}
    ~DevInfoGuard() {
        if (set != INVALID_HANDLE_VALUE) SetupDiDestroyDeviceInfoList(set);
    }
    DevInfoGuard(const DevInfoGuard&) = delete;
    DevInfoGuard& operator=(const DevInfoGuard&) = delete;
};

struct HidPreparsedGuard {
    PHIDP_PREPARSED_DATA ppd = nullptr;
    ~HidPreparsedGuard() {
        if (ppd) HidD_FreePreparsedData(ppd);
    }
    HidPreparsedGuard() = default;
    HidPreparsedGuard(const HidPreparsedGuard&) = delete;
    HidPreparsedGuard& operator=(const HidPreparsedGuard&) = delete;
};

struct RazerPidSpec {
    USHORT pid;
    uint8_t txId;
    DeviceKind kind;
    const wchar_t* name;
};

constexpr RazerPidSpec kRazerPids[] = {
    {0x00A5, 0x1F, DeviceKind::Mouse, L"Razer Viper V2 Pro"},
    {0x00A6, 0x1F, DeviceKind::Mouse, L"Razer Viper V2 Pro"},
    {0x00C0, 0x1F, DeviceKind::Mouse, L"Razer Viper V3 Pro"},
    {0x00C1, 0x1F, DeviceKind::Mouse, L"Razer Viper V3 Pro"},
    {0x00B6, 0x1F, DeviceKind::Mouse, L"Razer DeathAdder V3 Pro"},
    {0x00B7, 0x1F, DeviceKind::Mouse, L"Razer DeathAdder V3 Pro"},
    {0x00AA, 0x1F, DeviceKind::Mouse, L"Razer Basilisk V3 Pro"},
    {0x00AB, 0x1F, DeviceKind::Mouse, L"Razer Basilisk V3 Pro"},
    {0x00AF, 0x1F, DeviceKind::Mouse, L"Razer Cobra Pro"},
    {0x00B0, 0x1F, DeviceKind::Mouse, L"Razer Cobra Pro"},
    {0x00A7, 0x1F, DeviceKind::Mouse, L"Razer Naga V2 Pro"},
    {0x00A8, 0x1F, DeviceKind::Mouse, L"Razer Naga V2 Pro"},
    {0x00B8, 0x1F, DeviceKind::Mouse, L"Razer Viper V3 HyperSpeed"},
    {0x00C2, 0x1F, DeviceKind::Mouse, L"Razer DeathAdder V3 HyperSpeed"},
    {0x00C3, 0x1F, DeviceKind::Mouse, L"Razer DeathAdder V3 HyperSpeed"},
    {0x007A, 0x3F, DeviceKind::Mouse, L"Razer Viper Ultimate"},
    {0x007B, 0x3F, DeviceKind::Mouse, L"Razer Viper Ultimate"},
    {0x007C, 0x3F, DeviceKind::Mouse, L"Razer DeathAdder V2 Pro"},
    {0x007D, 0x3F, DeviceKind::Mouse, L"Razer DeathAdder V2 Pro"},
    {0x0086, 0x3F, DeviceKind::Mouse, L"Razer Basilisk Ultimate"},
    {0x0088, 0x3F, DeviceKind::Mouse, L"Razer Basilisk Ultimate"},
    {0x0094, 0x3F, DeviceKind::Mouse, L"Razer Orochi V2"},
    {0x025A, 0x3F, DeviceKind::Keyboard, L"Razer BlackWidow V3 Pro"},
    {0x025C, 0x3F, DeviceKind::Keyboard, L"Razer BlackWidow V3 Pro"},
};

const RazerPidSpec* LookupRazerPid(USHORT pid) {
    for (const auto& spec : kRazerPids) {
        if (spec.pid == pid) return &spec;
    }
    return nullptr;
}

struct CompxPidSpec {
    USHORT vid;
    USHORT pid;
    const wchar_t* name;
};

constexpr CompxPidSpec kCompxPids[] = {
    {0x3554, 0xF503, L"VGN Dragonfly F1 Pro"},
    {0x3554, 0xFB3E, L"VGN F2 Pro Max"},
    {0x3554, 0xF58A, L"VXE R1 Pro Max"},
    {0x3554, 0xF58E, L"VXE R1 SE+"},
    {0x373B, 0x1085, L"VXE R1 SE+"},
    {0x373B, 0x104D, L"VXE MAD R"},
    {0x373B, 0x1040, L"VXE MAD R"},
    {0x373B, 0x1031, L"ATK F1 Ultimate"},
    {0x373B, 0x11D9, L"ATK A9 Ultimate"},
    {0x3554, 0xF524, L"Zaopin Z2 Mini"},
    {0x3554, 0xF5F7, L"Scyrox V8"},
    {0x260D, 0x1175, L"Dareu A950"},
    {0x260D, 0x1154, L"Dareu A950"},
};

const CompxPidSpec* LookupCompxPid(USHORT vid, USHORT pid) {
    for (const auto& spec : kCompxPids) {
        if (spec.vid == vid && spec.pid == pid) return &spec;
    }
    return nullptr;
}

bool IsLogitechReceiverPid(USHORT pid) {
    return pid == 0xC52B || pid == 0xC548 || pid == 0xC547 || pid == 0xC539 || pid == 0xC53F;
}

std::wstring ReadHidProductString(HANDLE h) {
    wchar_t buf[128]{};
    if (!HidD_GetProductString(h, buf, sizeof(buf) - sizeof(wchar_t))) return {};
    buf[127] = L'\0';
    return SanitizeDeviceText(buf, 64);
}

template <typename MatchFn>
bool HidTransactOverlapped(HANDLE h, const uint8_t* tx, size_t txLen,
                           uint8_t* rx, size_t rxLen, size_t minMatchLen,
                           MatchFn&& match, DWORD totalTimeoutMs = 150) {
    OVERLAPPED ovW{};
    ovW.hEvent = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    if (!ovW.hEvent) return false;

    DWORD written = 0;
    BOOL okW = WriteFile(h, tx, static_cast<DWORD>(txLen), &written, &ovW);
    if (!okW) {
        if (GetLastError() != ERROR_IO_PENDING) {
            CloseHandle(ovW.hEvent);
            return false;
        }
        if (WaitForSingleObject(ovW.hEvent, totalTimeoutMs) != WAIT_OBJECT_0) {
            CancelIoEx(h, &ovW);
            GetOverlappedResult(h, &ovW, &written, TRUE);
            CloseHandle(ovW.hEvent);
            return false;
        }
        if (!GetOverlappedResult(h, &ovW, &written, FALSE)) {
            CloseHandle(ovW.hEvent);
            return false;
        }
    }
    CloseHandle(ovW.hEvent);

    const ULONGLONG deadline = GetTickCount64() + totalTimeoutMs;
    OVERLAPPED ovR{};
    ovR.hEvent = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    if (!ovR.hEvent) return false;

    bool matched = false;
    while (GetTickCount64() < deadline) {
        ResetEvent(ovR.hEvent);
        DWORD readBytes = 0;
        BOOL okR = ReadFile(h, rx, static_cast<DWORD>(rxLen), &readBytes, &ovR);
        if (!okR) {
            if (GetLastError() != ERROR_IO_PENDING) break;
            const ULONGLONG now = GetTickCount64();
            const DWORD rem = (now < deadline) ? static_cast<DWORD>(deadline - now) : 0;
            if (WaitForSingleObject(ovR.hEvent, rem) != WAIT_OBJECT_0) {
                CancelIoEx(h, &ovR);
                GetOverlappedResult(h, &ovR, &readBytes, TRUE);
                break;
            }
            if (!GetOverlappedResult(h, &ovR, &readBytes, FALSE)) break;
        }
        if (readBytes >= minMatchLen && match(rx, readBytes)) {
            matched = true;
            break;
        }
    }
    CloseHandle(ovR.hEvent);
    return matched;
}

bool HidppCall(HANDLE h, size_t outCap, size_t inCap, uint8_t devIdx, uint8_t featIdx,
               uint8_t funcId, const uint8_t* params, size_t paramsLen,
               std::vector<uint8_t>& resp, bool& isProtocolError) {
    isProtocolError = false;
    std::vector<uint8_t> tx(std::max<size_t>(outCap, 20), 0);
    tx[0] = 0x11;
    tx[1] = devIdx;
    tx[2] = featIdx;
    tx[3] = static_cast<uint8_t>((funcId << 4) | kHidppSwId);
    if (params && paramsLen > 0) {
        const size_t copyLen = std::min<size_t>(paramsLen, 16);
        std::memcpy(tx.data() + 4, params, copyLen);
    }

    resp.assign(std::max<size_t>(inCap, 20), 0);
    const bool ok = HidTransactOverlapped(
        h, tx.data(), tx.size(), resp.data(), resp.size(), 7,
        [devIdx, featIdx](const uint8_t* r, size_t n) {
            if (r[0] == 0x10 && n >= 7 && r[1] == devIdx && (r[2] == 0x8F || r[2] == 0xFF)) {
                return true;
            }
            if (r[0] == 0x11 && n >= 20 && r[1] == devIdx) {
                if (r[2] == 0xFF || r[2] == 0x8F) return true;
                return r[2] == featIdx && (r[3] & 0x0F) == kHidppSwId;
            }
            return false;
        });
    if (!ok) return false;
    if (resp[2] == 0xFF || resp[2] == 0x8F) {
        isProtocolError = true;
        return false;
    }
    return true;
}

uint8_t HidppFindFeature(HANDLE h, size_t outCap, size_t inCap, uint8_t devIdx,
                         uint16_t featId, bool& isProtocolError) {
    uint8_t params[2] = {static_cast<uint8_t>(featId >> 8), static_cast<uint8_t>(featId & 0xFF)};
    std::vector<uint8_t> resp;
    if (!HidppCall(h, outCap, inCap, devIdx, 0x00, 0x00, params, 2, resp, isProtocolError)) {
        return 0;
    }
    return resp[4];
}

int VoltageToPercent(int mv) {
    struct Point { int mv; int pct; };
    static constexpr Point kLiIon[] = {
        {4186, 100}, {4067, 90}, {3989, 80}, {3922, 70}, {3859, 60}, {3811, 50},
        {3783, 40},  {3763, 30}, {3743, 20}, {3706, 10}, {3663, 5},  {3500, 0}
    };
    static constexpr Point kAa[] = {
        {1500, 100}, {1400, 85}, {1300, 65}, {1250, 45}, {1200, 25}, {1100, 10}, {1000, 0}
    };

    const Point* pts = nullptr;
    size_t count = 0;
    if (mv >= 2500) {
        pts = kLiIon;
        count = sizeof(kLiIon) / sizeof(kLiIon[0]);
    } else if (mv >= 900) {
        pts = kAa;
        count = sizeof(kAa) / sizeof(kAa[0]);
    } else {
        return -1;
    }

    if (mv >= pts[0].mv) return pts[0].pct;
    for (size_t i = 1; i < count; ++i) {
        if (mv >= pts[i].mv) {
            const int dm = pts[i - 1].mv - pts[i].mv;
            const int dp = pts[i - 1].pct - pts[i].pct;
            if (dm <= 0) return pts[i].pct;
            return pts[i].pct + ((mv - pts[i].mv) * dp + dm / 2) / dm;
        }
    }
    return 0;
}

bool QueryHidppBattery(HANDLE h, size_t outCap, size_t inCap, uint8_t devIdx,
                       uint16_t featId, uint8_t featIdx, int& outPct, bool& outCharging,
                       bool& isProtocolError) {
    std::vector<uint8_t> resp;
    const uint8_t funcId = (featId == 0x1004) ? 0x01 : 0x00;
    if (!HidppCall(h, outCap, inCap, devIdx, featIdx, funcId, nullptr, 0, resp, isProtocolError)) {
        return false;
    }

    if (featId == 0x1004) {
        int pct = resp[4];
        if (pct == 0) {
            const uint8_t bits = resp[5];
            if (bits & 0x08) pct = 90;
            else if (bits & 0x04) pct = 50;
            else if (bits & 0x02) pct = 20;
            else if (bits & 0x01) pct = 5;
        }
        if (pct < 0 || pct > 100) return false;
        outPct = pct;
        outCharging = (resp[6] >= 1 && resp[6] <= 4);
        return true;
    }

    if (featId == 0x1000) {
        if (resp[4] > 100) return false;
        outPct = resp[4];
        outCharging = (resp[6] >= 1 && resp[6] <= 4);
        return true;
    }

    if (featId == 0x1001) {
        const int mv = (static_cast<int>(resp[4]) << 8) | resp[5];
        const int pct = VoltageToPercent(mv);
        if (pct < 0) return false;
        outPct = pct;
        outCharging = (resp[6] & 0x80) != 0;
        return true;
    }

    return false;
}

void QueryHidppDeviceInfo(HANDLE h, size_t outCap, size_t inCap, uint8_t devIdx,
                          std::wstring& outName, DeviceKind& outKind) {
    bool protoErr = false;
    const uint8_t nameFeat = HidppFindFeature(h, outCap, inCap, devIdx, 0x0005, protoErr);
    if (nameFeat == 0) return;

    std::vector<uint8_t> resp;
    if (HidppCall(h, outCap, inCap, devIdx, nameFeat, 0x00, nullptr, 0, resp, protoErr)) {
        const size_t nameLen = std::min<size_t>(resp[4], 64);
        std::string rawName;
        rawName.reserve(nameLen);
        while (rawName.size() < nameLen) {
            const uint8_t off = static_cast<uint8_t>(rawName.size());
            if (!HidppCall(h, outCap, inCap, devIdx, nameFeat, 0x01, &off, 1, resp, protoErr)) break;
            const size_t remain = nameLen - rawName.size();
            const size_t chunk = std::min<size_t>(remain, 16);
            for (size_t k = 0; k < chunk; ++k) {
                const char ch = static_cast<char>(resp[4 + k]);
                if (ch == '\0') break;
                rawName.push_back(ch);
            }
            if (chunk == 0) break;
        }
        if (!rawName.empty()) {
            const int need = MultiByteToWideChar(CP_UTF8, 0, rawName.data(), static_cast<int>(rawName.size()), nullptr, 0);
            if (need > 0) {
                std::wstring w(static_cast<size_t>(need), L'\0');
                MultiByteToWideChar(CP_UTF8, 0, rawName.data(), static_cast<int>(rawName.size()), w.data(), need);
                outName = SanitizeDeviceText(w, 64);
            }
        }
    }

    if (HidppCall(h, outCap, inCap, devIdx, nameFeat, 0x02, nullptr, 0, resp, protoErr)) {
        switch (resp[4]) {
            case 0: outKind = DeviceKind::Keyboard; break;
            case 3: outKind = DeviceKind::Mouse; break;
            case 8: outKind = DeviceKind::Headphones; break;
            default: break;
        }
    }
}

bool HidppPing(HANDLE h, size_t outCap, size_t inCap, uint8_t devIdx, bool& isProtocolError) {
    uint8_t params[3] = {0x00, 0x00, 0xAA};
    std::vector<uint8_t> resp;
    if (!HidppCall(h, outCap, inCap, devIdx, 0x00, 0x01, params, 3, resp, isProtocolError)) {
        return false;
    }
    return resp.size() >= 20 && resp[6] == 0xAA && resp[4] >= 2;
}

struct HidppCacheEntry {
    bool pinged = false;
    uint16_t batFeatId = 0;
    uint8_t batFeatIdx = 0;
    std::wstring name;
    DeviceKind kind = DeviceKind::Unknown;
};

std::mutex g_hidppCacheMtx;
std::unordered_map<std::wstring, HidppCacheEntry> g_hidppCache;

std::wstring MakeHidppCacheKey(const std::wstring& path, uint8_t devIdx) {
    return ToLower(path) + L"#" + std::to_wstring(devIdx);
}

void ProbeLogitechHidpp(std::array<DeviceState, 3>& out, const std::wstring& path,
                        USHORT pid, size_t outCap, size_t inCap) {
    HandleGuard h(CreateFileW(path.c_str(), GENERIC_READ | GENERIC_WRITE,
                              FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
                              OPEN_EXISTING, FILE_FLAG_OVERLAPPED, nullptr));
    if (h.h == INVALID_HANDLE_VALUE) return;

    const std::wstring productFallback = ReadHidProductString(h.h);
    const bool isReceiver = IsLogitechReceiverPid(pid);

    std::vector<uint8_t> targets;
    targets.push_back(0xFF);
    if (isReceiver) {
        for (uint8_t idx = 1; idx <= 6; ++idx) targets.push_back(idx);
    }

    for (uint8_t devIdx : targets) {
        const std::wstring cacheKey = MakeHidppCacheKey(path, devIdx);
        HidppCacheEntry cached{};
        bool hasCached = false;
        {
            std::lock_guard<std::mutex> lk(g_hidppCacheMtx);
            auto it = g_hidppCache.find(cacheKey);
            if (it != g_hidppCache.end()) {
                cached = it->second;
                hasCached = true;
            }
        }

        int pct = -1;
        bool charging = false;
        bool protoErr = false;

        if (hasCached && cached.pinged) {
            if (QueryHidppBattery(h.h, outCap, inCap, devIdx, cached.batFeatId, cached.batFeatIdx,
                                  pct, charging, protoErr)) {
                DeviceEntry entry{};
                entry.name = cached.name.empty() ? productFallback : cached.name;
                entry.percent = pct;
                entry.charging = charging;
                entry.connected = true;
                DeviceKind k = cached.kind;
                if (k == DeviceKind::Unknown) k = ClassifyDevice(entry.name);
                if (k == DeviceKind::Unknown) k = DeviceKind::Mouse;
                StoreCandidate(out, k, entry);
                continue;
            }
            std::lock_guard<std::mutex> lk(g_hidppCacheMtx);
            g_hidppCache.erase(cacheKey);
            continue;
        }

        if (!HidppPing(h.h, outCap, inCap, devIdx, protoErr)) {
            continue;
        }

        static constexpr uint16_t kBatFeats[] = {0x1004, 0x1000, 0x1001};
        uint16_t foundFeatId = 0;
        uint8_t foundFeatIdx = 0;
        for (uint16_t fid : kBatFeats) {
            const uint8_t fidx = HidppFindFeature(h.h, outCap, inCap, devIdx, fid, protoErr);
            if (protoErr) break;
            if (fidx != 0) {
                foundFeatId = fid;
                foundFeatIdx = fidx;
                break;
            }
        }
        if (protoErr || foundFeatIdx == 0) continue;

        if (!QueryHidppBattery(h.h, outCap, inCap, devIdx, foundFeatId, foundFeatIdx,
                               pct, charging, protoErr)) {
            continue;
        }

        std::wstring devName;
        DeviceKind devKind = DeviceKind::Unknown;
        QueryHidppDeviceInfo(h.h, outCap, inCap, devIdx, devName, devKind);
        if (devName.empty() && !isReceiver) devName = productFallback;
        if (devName.empty()) devName = L"Logitech Device";
        if (devKind == DeviceKind::Unknown) devKind = ClassifyDevice(devName);
        if (devKind == DeviceKind::Unknown) devKind = DeviceKind::Mouse;

        {
            std::lock_guard<std::mutex> lk(g_hidppCacheMtx);
            if (g_hidppCache.size() >= 64) g_hidppCache.clear();
            g_hidppCache[cacheKey] = {true, foundFeatId, foundFeatIdx, devName, devKind};
        }

        DeviceEntry entry{};
        entry.name = devName;
        entry.percent = pct;
        entry.charging = charging;
        entry.connected = true;
        StoreCandidate(out, devKind, entry);

        if (!isReceiver && devIdx == 0xFF) break;
    }
}

uint8_t RazerCalcCrc(const uint8_t* buf) {
    uint8_t crc = 0;
    for (size_t i = 3; i <= 88; ++i) crc ^= buf[i];
    return crc;
}

bool RazerFeatureQuery(HANDLE h, uint8_t txId, uint8_t cmdId, uint8_t* resp) {
    uint8_t req[91]{};
    req[0] = 0x00;
    req[1] = 0x00;
    req[2] = txId;
    req[6] = 0x02;
    req[7] = 0x07;
    req[8] = cmdId;
    req[89] = RazerCalcCrc(req);

    if (!HidD_SetFeature(h, req, sizeof(req))) return false;
    Sleep(35);

    std::memset(resp, 0, 91);
    resp[0] = 0x00;
    if (!HidD_GetFeature(h, resp, 91)) return false;

    return resp[1] == 0x02 && resp[2] == txId && resp[7] == 0x07 && resp[8] == cmdId && resp[89] == RazerCalcCrc(resp);
}

void ProbeRazerDevice(std::array<DeviceState, 3>& out, HANDLE h, const RazerPidSpec& spec) {
    uint8_t resp[91]{};
    if (!RazerFeatureQuery(h, spec.txId, 0x80, resp)) return;

    const int pct = (static_cast<int>(resp[10]) * 100 + 127) / 255;
    if (pct < 0 || pct > 100) return;

    bool charging = false;
    uint8_t chResp[91]{};
    if (RazerFeatureQuery(h, spec.txId, 0x84, chResp)) {
        charging = (chResp[10] != 0);
    }

    std::wstring name = ReadHidProductString(h);
    if (name.empty()) name = spec.name;

    DeviceEntry entry{};
    entry.name = name;
    entry.percent = pct;
    entry.charging = charging;
    entry.connected = true;
    StoreCandidate(out, spec.kind, entry);
}

void ProbeCompxDevice(std::array<DeviceState, 3>& out, const std::wstring& path,
                      const CompxPidSpec& spec, size_t outCap, size_t inCap) {
    HandleGuard h(CreateFileW(path.c_str(), GENERIC_READ | GENERIC_WRITE,
                              FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr,
                              OPEN_EXISTING, FILE_FLAG_OVERLAPPED, nullptr));
    if (h.h == INVALID_HANDLE_VALUE) return;

    std::vector<uint8_t> tx(std::max<size_t>(outCap, 17), 0);
    tx[0] = 0x08;
    tx[1] = 0x04;
    tx[16] = 0x49;

    std::vector<uint8_t> rx(std::max<size_t>(inCap, 17), 0);
    const bool ok = HidTransactOverlapped(
        h.h, tx.data(), tx.size(), rx.data(), rx.size(), 17,
        [](const uint8_t* r, size_t n) {
            if (n < 17 || r[0] != 0x08 || r[1] != 0x04) return false;
            uint32_t sum = 0;
            for (size_t i = 0; i < 17; ++i) sum += r[i];
            return (sum & 0xFFu) == 0x55u;
        });
    if (!ok || rx[6] > 100) return;

    const int pct = rx[6];
    const bool charging = (rx[7] != 0 && pct < 100);
    std::wstring name = ReadHidProductString(h.h);
    if (name.empty()) name = spec.name;

    DeviceEntry entry{};
    entry.name = name;
    entry.percent = pct;
    entry.charging = charging;
    entry.connected = true;
    StoreCandidate(out, DeviceKind::Mouse, entry);
}

} // namespace

void ScanVendorHidDevices(std::array<DeviceState, 3>& out) {
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

        if (!SetupDiGetDeviceInterfaceDetailW(set.set, &ifData, detail, reqSize, nullptr, nullptr)) continue;

        HandleGuard probeH(CreateFileW(detail->DevicePath, 0,
                                       FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0, nullptr));
        if (probeH.h == INVALID_HANDLE_VALUE) continue;

        HIDD_ATTRIBUTES attrs{};
        attrs.Size = sizeof(attrs);
        if (!HidD_GetAttributes(probeH.h, &attrs)) continue;

        HidPreparsedGuard ppd;
        if (!HidD_GetPreparsedData(probeH.h, &ppd.ppd) || !ppd.ppd) continue;

        HIDP_CAPS caps{};
        if (HidP_GetCaps(ppd.ppd, &caps) != HIDP_STATUS_SUCCESS) continue;

        if (attrs.VendorID == kVidLogitech && caps.UsagePage == 0xFF00 &&
            (caps.Usage == 0x0001 || caps.Usage == 0x0002) &&
            caps.InputReportByteLength >= 20 && caps.OutputReportByteLength >= 20) {
            ProbeLogitechHidpp(out, detail->DevicePath, attrs.ProductID,
                               caps.OutputReportByteLength, caps.InputReportByteLength);
            continue;
        }

        if (attrs.VendorID == kVidRazer && caps.FeatureReportByteLength == 91) {
            if (const RazerPidSpec* spec = LookupRazerPid(attrs.ProductID)) {
                ProbeRazerDevice(out, probeH.h, *spec);
            }
            continue;
        }

        if (caps.UsagePage == 0xFF02 && caps.Usage == 0x0002 &&
            caps.InputReportByteLength >= 17 && caps.OutputReportByteLength >= 17) {
            if (const CompxPidSpec* spec = LookupCompxPid(attrs.VendorID, attrs.ProductID)) {
                ProbeCompxDevice(out, detail->DevicePath, *spec,
                                 caps.OutputReportByteLength, caps.InputReportByteLength);
            }
        }
    }
}
