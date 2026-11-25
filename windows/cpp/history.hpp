#pragma once

#include "device.hpp"

#include <array>
#include <cstdint>
#include <string>
#include <vector>

struct HistoryItem {
    DeviceKind kind = DeviceKind::Unknown;
    std::wstring key;
    std::wstring name;
    int percent = -1;
    int64_t lastSeenUnix = 0;
};

class DeviceHistory {
public:
    void Load();
    void SyncActive(const std::array<DeviceState, 3>& active);
    std::vector<HistoryItem> GetInactiveForKind(DeviceKind kind, const std::vector<DeviceEntry>& activeItems) const;
    bool RemoveEntry(DeviceKind kind, const std::wstring& key);
    bool ClearInactiveForKind(DeviceKind kind, const std::vector<DeviceEntry>& activeItems);

    static std::wstring FormatRelativeTime(int64_t lastSeenUnix);

private:
    void Save() const;
    static std::wstring GetStorageDir();
    static std::wstring GetStoragePath();

    std::vector<HistoryItem> items_;
};