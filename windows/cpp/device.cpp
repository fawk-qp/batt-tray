#include "device.hpp"

#include <algorithm>
#include <cwctype>
#include <initializer_list>

namespace {

bool ContainsAny(const std::wstring& hay, std::initializer_list<const wchar_t*> needles) {
    for (const wchar_t* n : needles) {
        if (hay.find(n) != std::wstring::npos) return true;
    }
    return false;
}

bool StripSuffixI(std::wstring& s, const wchar_t* suffix) {
    const std::wstring low = ToLower(s);
    const std::wstring suf = ToLower(suffix);
    if (low.size() < suf.size()) return false;
    if (low.compare(low.size() - suf.size(), suf.size(), suf) != 0) return false;
    s.erase(s.size() - suf.size());
    s = Trim(std::move(s));
    while (!s.empty() && (s.back() == L'-' || s.back() == L':' || s.back() == L'–')) {
        s.pop_back();
        s = Trim(std::move(s));
    }
    return true;
}

} // namespace

const std::array<SlotSpec, 3>& GetSlotSpecs() {
    static const std::array<SlotSpec, 3> kSpecs = {{
        {DeviceKind::Mouse,      1001, L"Мышь",       L'\uE962'},
        {DeviceKind::Keyboard,   1002, L"Клавиатура", L'\uE765'},
        {DeviceKind::Headphones, 1003, L"Наушники",   L'\uE7F6'},
    }};
    return kSpecs;
}

std::wstring ToLower(std::wstring s) {
    for (wchar_t& c : s) {
        c = static_cast<wchar_t>(std::towlower(c));
    }
    return s;
}

std::wstring Trim(std::wstring s) {
    while (!s.empty() && std::iswspace(s.back())) s.pop_back();
    size_t i = 0;
    while (i < s.size() && std::iswspace(s[i])) ++i;
    return s.substr(i);
}

std::wstring SanitizeDeviceText(const std::wstring& s, size_t maxChars) {
    std::wstring clean;
    clean.reserve(std::min(s.size(), maxChars));
    for (wchar_t ch : s) {
        if (ch < 0x20 || ch == 0x7F) continue;
        if (ch == 0x061C || (ch >= 0x200B && ch <= 0x200F) || (ch >= 0x202A && ch <= 0x202E) ||
            (ch >= 0x2066 && ch <= 0x2069) || ch == 0xFEFF) continue;
        clean.push_back(ch);
        if (clean.size() >= maxChars) break;
    }
    return Trim(std::move(clean));
}

std::wstring CleanDeviceName(std::wstring s) {
    s = SanitizeDeviceText(s, 80);
    static constexpr const wchar_t* kSuffixes[] = {
        L"hands-free ag audio",
        L"hands-free ag",
        L"hands-free audio",
        L"hands-free",
        L"avrcp transport",
        L"stereo",
    };
    bool changed = true;
    while (changed) {
        changed = false;
        for (const wchar_t* suf : kSuffixes) {
            if (StripSuffixI(s, suf)) changed = true;
        }
    }
    return SanitizeDeviceText(s, 64);
}

bool NamesMatchAudio(const std::wstring& devName, const std::wstring& audioEndpointName) {
    const std::wstring dev = ToLower(CleanDeviceName(devName));
    const std::wstring audio = ToLower(CleanDeviceName(audioEndpointName));
    if (dev.size() < 4 || audio.size() < 4) return false;
    if (audio.find(dev) != std::wstring::npos || dev.find(audio) != std::wstring::npos) return true;

    const size_t open = audio.find(L'(');
    const size_t close = audio.rfind(L')');
    if (open != std::wstring::npos && close != std::wstring::npos && close > open + 4) {
        const std::wstring inner = ToLower(CleanDeviceName(audio.substr(open + 1, close - open - 1)));
        if (inner.size() >= 4 && (dev.find(inner) != std::wstring::npos || inner.find(dev) != std::wstring::npos)) {
            return true;
        }
    }
    return false;
}

DeviceKind ClassifyDevice(const std::wstring& name, const std::wstring& pnpClass, uint32_t cod) {
    const std::wstring low = ToLower(name);
    if (ContainsAny(low, {L"gamepad", L"controller", L"xbox", L"dualsense", L"dualshock", L"joystick", L"joy-con"})) {
        return DeviceKind::Unknown;
    }

    if (cod != 0) {
        const uint32_t major = (cod >> 8) & 0x1Fu;
        if (major == 0x04u) return DeviceKind::Headphones;
        if (major == 0x05u) {
            if (cod & 0x80u) return DeviceKind::Mouse;
            if (cod & 0x40u) return DeviceKind::Keyboard;
        }
    }

    if (ContainsAny(low, {
            L"keyboard", L"kbd", L"keychron", L"nuphy", L"akko", L"wooting", L"anne pro", L"rk61", L"rk84",
            L"mx keys", L"k380", L"k780", L"g915", L"g715", L"apex pro", L"blackwidow", L"huntsman",
            L"magic keyboard", L"клавиатур"})) {
        return DeviceKind::Keyboard;
    }

    if (ContainsAny(low, {
            L"mouse", L"mx master", L"mx anywhere", L"g pro", L"superlight", L"g305", L"g304", L"g502",
            L"viper", L"deathadder", L"basilisk", L"orochi", L"aerox", L"lamzu", L"pulsar",
            L"attack shark", L"vgn", L"m720", L"m650", L"pebble", L"trackball", L"мышь"})) {
        return DeviceKind::Mouse;
    }

    if (ContainsAny(low, {
            L"headphone", L"headset", L"earbud", L"earphone", L"airpods", L"buds", L"pods", L"hands-free",
            L"wh-1000", L"wf-1000", L"jbl", L"marshall", L"bose", L"qc35", L"qc45", L"sennheiser",
            L"momentum", L"beats", L"barracuda", L"blackshark", L"arctis", L"cloud", L"g733", L"g435",
            L"freebuds", L"soundcore", L"anker", L"edifier", L"sony", L"наушник", L"гарнитур"})) {
        return DeviceKind::Headphones;
    }

    const std::wstring cls = ToLower(pnpClass);
    if (cls == L"mouse") return DeviceKind::Mouse;
    if (cls == L"keyboard") return DeviceKind::Keyboard;
    if (cls == L"audioendpoint" || cls == L"media") return DeviceKind::Headphones;

    return DeviceKind::Unknown;
}

void StoreCandidate(std::array<DeviceState, 3>& out, DeviceKind kind, const DeviceEntry& entry) {
    if (kind == DeviceKind::Unknown || entry.percent < 0 || entry.percent > 100) return;

    DeviceEntry clean = entry;
    clean.name = CleanDeviceName(clean.name);
    clean.containerId = SanitizeDeviceText(ToLower(clean.containerId), 48);
    const std::wstring normName = ToLower(clean.name);
    clean.key = !clean.containerId.empty() ? clean.containerId : normName;
    if (clean.key.empty()) return;

    auto& items = out[static_cast<size_t>(kind)].items;
    for (auto& existing : items) {
        const bool sameCid = !clean.containerId.empty() && clean.containerId == existing.containerId;
        const bool sameName = !normName.empty() && normName == ToLower(existing.name);
        if (sameCid || sameName) {
            if (clean.activeAudio) existing.activeAudio = true;
            if (clean.connected) existing.connected = true;
            if (clean.charging) existing.charging = true;
            if (clean.bthAddr != 0 && existing.bthAddr == 0) existing.bthAddr = clean.bthAddr;
            if (existing.name.empty() && !clean.name.empty()) existing.name = clean.name;
            if (existing.key.empty()) existing.key = clean.key;
            return;
        }
    }
    if (items.size() >= 16) return;
    items.push_back(std::move(clean));
}

void FinalizeDevices(std::array<DeviceState, 3>& out) {
    for (auto& slot : out) {
        if (slot.items.empty()) {
            slot.online = false;
            slot.percent = -1;
            slot.charging = false;
            slot.name.clear();
            continue;
        }
        std::stable_sort(slot.items.begin(), slot.items.end(), [](const DeviceEntry& a, const DeviceEntry& b) {
            if (a.activeAudio != b.activeAudio) return a.activeAudio > b.activeAudio;
            if (a.connected != b.connected) return a.connected > b.connected;
            return false;
        });
        slot.online = true;
        slot.name = slot.items.front().name;
        slot.percent = slot.items.front().percent;
        slot.charging = slot.items.front().charging;
    }
}