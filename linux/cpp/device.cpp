#include "device.hpp"

#include <cctype>
#include <initializer_list>

namespace {

bool ContainsAny(const std::string& hay, std::initializer_list<const char*> needles) {
    for (const char* n : needles) {
        if (hay.find(n) != std::string::npos) return true;
    }
    return false;
}

} // namespace

const std::array<SlotSpec, 3>& GetSlotSpecs() {
    static const std::array<SlotSpec, 3> kSpecs = {{
        {DeviceKind::Mouse,      "batt-tray-mouse",      "Мышь",       "input-mouse-symbolic"},
        {DeviceKind::Keyboard,   "batt-tray-keyboard",   "Клавиатура", "input-keyboard-symbolic"},
        {DeviceKind::Headphones, "batt-tray-headphones", "Наушники",   "audio-headphones-symbolic"},
    }};
    return kSpecs;
}

std::string ToLower(std::string s) {
    for (char& c : s) {
        c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
    }
    return s;
}

std::string Trim(std::string s) {
    while (!s.empty() && std::isspace(static_cast<unsigned char>(s.back()))) s.pop_back();
    size_t i = 0;
    while (i < s.size() && std::isspace(static_cast<unsigned char>(s[i]))) ++i;
    return s.substr(i);
}

DeviceKind ClassifyDevice(const std::string& name, const std::string& iconHint, uint32_t cod) {
    // Bluetooth Class of Device: Major 0x04 = Audio, 0x05 = Peripheral (0x80 mouse, 0x40 keyboard)
    if (cod != 0) {
        const uint32_t major = (cod >> 8) & 0x1Fu;
        if (major == 0x04u) return DeviceKind::Headphones;
        if (major == 0x05u) {
            if (cod & 0x80u) return DeviceKind::Mouse;
            if (cod & 0x40u) return DeviceKind::Keyboard;
        }
    }

    const std::string ic = ToLower(iconHint);
    if (ic.find("mouse") != std::string::npos) return DeviceKind::Mouse;
    if (ic.find("keyboard") != std::string::npos) return DeviceKind::Keyboard;
    if (ic.find("head") != std::string::npos || ic.find("audio") != std::string::npos) return DeviceKind::Headphones;

    const std::string low = ToLower(name);
    if (ContainsAny(low, {"gamepad", "controller", "xbox", "dualsense", "dualshock", "joystick", "joy-con"})) {
        return DeviceKind::Unknown;
    }

    if (ContainsAny(low, {
            "keyboard", "kbd", "keychron", "nuphy", "akko", "wooting", "anne pro", "rk61", "rk84",
            "mx keys", "k380", "k780", "g915", "g715", "apex pro", "blackwidow", "huntsman",
            "magic keyboard", "клавиатур"})) {
        return DeviceKind::Keyboard;
    }

    if (ContainsAny(low, {
            "mouse", "mx master", "mx anywhere", "g pro", "superlight", "g305", "g304", "g502",
            "viper", "deathadder", "basilisk", "orochi", "aerox", "lamzu", "pulsar",
            "attack shark", "vgn", "m720", "m650", "pebble", "trackball", "мышь"})) {
        return DeviceKind::Mouse;
    }

    if (ContainsAny(low, {
            "headphone", "headset", "earbud", "earphone", "airpods", "buds", "pods", "hands-free",
            "wh-1000", "wf-1000", "jbl", "marshall", "bose", "qc35", "qc45", "sennheiser",
            "momentum", "beats", "barracuda", "blackshark", "arctis", "cloud", "g733", "g435",
            "freebuds", "soundcore", "anker", "edifier", "sony", "наушник", "гарнитур"})) {
        return DeviceKind::Headphones;
    }

    return DeviceKind::Unknown;
}

void StoreCandidate(std::array<DeviceState, 3>& out, DeviceKind kind, const std::string& name, int percent, bool charging) {
    if (kind == DeviceKind::Unknown || percent < 0 || percent > 100) return;
    auto& dst = out[static_cast<size_t>(kind)];
    if (!dst.online) {
        dst.name = Trim(name);
        dst.percent = percent;
        dst.online = true;
        dst.charging = charging;
    }
}