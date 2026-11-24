#include "device.hpp"

#include <cwctype>
#include <initializer_list>

namespace {

bool ContainsAny(const std::wstring& hay, std::initializer_list<const wchar_t*> needles) {
    for (const wchar_t* n : needles) {
        if (hay.find(n) != std::wstring::npos) return true;
    }
    return false;
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

DeviceKind ClassifyDevice(const std::wstring& name, const std::wstring& pnpClass, uint32_t cod) {
    const std::wstring low = ToLower(name);
    if (ContainsAny(low, {L"gamepad", L"controller", L"xbox", L"dualsense", L"dualshock", L"joystick", L"joy-con"})) {
        return DeviceKind::Unknown;
    }

    // Bluetooth Class of Device: Major 0x04 = Audio, 0x05 = Peripheral (0x80 mouse, 0x40 keyboard)
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

void StoreCandidate(std::array<DeviceState, 3>& out, DeviceKind kind, const std::wstring& name, int percent, bool charging) {
    if (kind == DeviceKind::Unknown || percent < 0 || percent > 100) return;
    auto& dst = out[static_cast<size_t>(kind)];
    if (!dst.online) {
        dst.name = Trim(name);
        dst.percent = percent;
        dst.online = true;
        dst.charging = charging;
    }
}