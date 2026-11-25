#pragma once

#include <array>
#include <cstdint>
#include <string>
#include <vector>

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>

enum class DeviceKind : int {
    Mouse = 0,
    Keyboard = 1,
    Headphones = 2,
    Unknown = 3,
};

struct DeviceEntry {
    std::wstring key;
    std::wstring name;
    std::wstring containerId;
    int percent = -1;
    bool charging = false;
    bool activeAudio = false;
    bool connected = false;
};

struct DeviceState {
    std::wstring name;
    int percent = -1;
    bool online = false;
    bool charging = false;
    std::vector<DeviceEntry> items;
};

struct SlotSpec {
    DeviceKind kind;
    UINT uid;
    const wchar_t* label;
    wchar_t glyph;
};

const std::array<SlotSpec, 3>& GetSlotSpecs();
std::wstring ToLower(std::wstring s);
std::wstring Trim(std::wstring s);
std::wstring SanitizeDeviceText(const std::wstring& s, size_t maxChars = 64);
std::wstring CleanDeviceName(std::wstring s);
bool NamesMatchAudio(const std::wstring& devName, const std::wstring& audioEndpointName);
DeviceKind ClassifyDevice(const std::wstring& name, const std::wstring& pnpClass = L"", uint32_t cod = 0);
void StoreCandidate(std::array<DeviceState, 3>& out, DeviceKind kind, const DeviceEntry& entry);
void FinalizeDevices(std::array<DeviceState, 3>& out);