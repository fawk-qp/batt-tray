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

// UTF-8 safe sanitizer: strips control chars (<0x20, 0x7F) and invalid byte sequences so D-Bus never rejects strings.
std::string SanitizeDeviceText(const std::string& s, size_t maxChars) {
    std::string out;
    out.reserve(s.size());
    size_t chars = 0;
    size_t i = 0;
    while (i < s.size() && chars < maxChars) {
        const unsigned char b = static_cast<unsigned char>(s[i]);
        if (b < 0x20u || b == 0x7Fu) {
            ++i;
            continue;
        }
        if ((b & 0x80u) == 0u) {
            out.push_back(static_cast<char>(b));
            ++i;
            ++chars;
            continue;
        }
        size_t len = 0;
        if ((b & 0xE0u) == 0xC0u && b >= 0xC2u) len = 2;
        else if ((b & 0xF0u) == 0xE0u) len = 3;
        else if ((b & 0xF8u) == 0xF0u && b <= 0xF4u) len = 4;
        else {
            ++i;
            continue;
        }
        if (i + len > s.size()) break;
        bool valid = true;
        for (size_t k = 1; k < len; ++k) {
            if ((static_cast<unsigned char>(s[i + k]) & 0xC0u) != 0x80u) {
                valid = false;
                break;
            }
        }
        if (!valid) {
            ++i;
            continue;
        }
        // Reject overlong forms, UTF-16 surrogates and code points beyond U+10FFFF.
        const unsigned char b1 = static_cast<unsigned char>(s[i + 1]);
        if ((b == 0xE0u && (b1 < 0xA0u || b1 > 0xBFu)) ||
            (b == 0xEDu && (b1 < 0x80u || b1 > 0x9Fu)) ||
            (b == 0xF0u && (b1 < 0x90u || b1 > 0xBFu)) ||
            (b == 0xF4u && (b1 < 0x80u || b1 > 0x8Fu))) {
            ++i;
            continue;
        }
        // Drop bidi controls and zero-width characters (U+061C, U+200B..200F, U+202A..202E, U+2066..2069, U+FEFF).
        if (len == 2 && b == 0xD8u && b1 == 0x9Cu) {
            i += len;
            continue;
        }
        if (len == 3) {
            const unsigned cp = ((b & 0x0Fu) << 12) |
                                ((static_cast<unsigned char>(s[i + 1]) & 0x3Fu) << 6) |
                                (static_cast<unsigned char>(s[i + 2]) & 0x3Fu);
            if ((cp >= 0x200Bu && cp <= 0x200Fu) || (cp >= 0x202Au && cp <= 0x202Eu) ||
                (cp >= 0x2066u && cp <= 0x2069u) || cp == 0xFEFFu) {
                i += len;
                continue;
            }
        }
        out.append(s, i, len);
        i += len;
        ++chars;
    }
    return Trim(std::move(out));
}

std::string StripBatteryDetailSuffix(const std::string& s) {
    const size_t paren = s.rfind(" (");
    if (paren != std::string::npos && s.size() >= 2 &&
        s.compare(s.size() - 2, 2, "%)") == 0) {
        return s.substr(0, paren);
    }
    return s;
}

DeviceKind ClassifyDevice(const std::string& name, const std::string& iconHint, uint32_t cod) {
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
            "mx keys", "k380", "k780", "k850", "ergo k860", "wave keys", "pop keys", "craft",
            "g915", "g715", "apex pro", "blackwidow", "huntsman",
            "magic keyboard", "клавиатур"})) {
        return DeviceKind::Keyboard;
    }

    if (ContainsAny(low, {
            "mouse", "mx master", "mx anywhere", "g pro", "pro wireless", "superlight",
            "g305", "g304", "g502", "g603", "g604", "g703", "g903",
            "viper", "deathadder", "basilisk", "orochi", "aerox", "lamzu", "pulsar",
            "attack shark", "vgn", "m720", "m705", "m650", "lift", "ergo m575",
            "pebble", "trackball", "мышь"})) {
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
    const std::string clean = SanitizeDeviceText(name, 64);
    const std::string key = ToLower(clean);

    auto& dst = out[static_cast<size_t>(kind)];
    if (!key.empty()) {
        bool exists = false;
        for (auto& item : dst.items) {
            if (item.key == key) {
                if (charging) item.charging = true;
                exists = true;
                break;
            }
        }
        if (!exists && dst.items.size() < 16) {
            dst.items.push_back({key, clean, percent, charging});
        }
    }

    if (!dst.online) {
        dst.name = clean;
        dst.percent = percent;
        dst.online = true;
        dst.charging = charging;
    }
}