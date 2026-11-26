#pragma once

#include <array>
#include <cstdint>
#include <string>
#include <vector>

enum class DeviceKind : int {
    Mouse = 0,
    Keyboard = 1,
    Headphones = 2,
    Unknown = 3,
};

struct DeviceEntry {
    std::string key;
    std::string name;
    int percent = -1;
    bool charging = false;
};

struct DeviceState {
    std::string name;
    int percent = -1;
    bool online = false;
    bool charging = false;
    std::vector<DeviceEntry> items;
};

struct SlotSpec {
    DeviceKind kind;
    const char* id;
    const char* label;
    const char* iconName;
};

const std::array<SlotSpec, 3>& GetSlotSpecs();
std::string ToLower(std::string s);
std::string Trim(std::string s);
std::string SanitizeDeviceText(const std::string& s, size_t maxChars = 64);
std::string StripBatteryDetailSuffix(const std::string& s);
DeviceKind ClassifyDevice(const std::string& name, const std::string& iconHint = "", uint32_t cod = 0);
void StoreCandidate(std::array<DeviceState, 3>& out, DeviceKind kind, const std::string& name, int percent, bool charging);
