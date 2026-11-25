#pragma once

#include "device.hpp"

#include <array>
#include <cstdint>
#include <string>
#include <vector>

struct HistoryItem {
    DeviceKind kind = DeviceKind::Unknown;
    std::string key;
    std::string name;
    int percent = -1;
    int64_t lastSeenUnix = 0;
};

class DeviceHistory {
public:
    void Load(bool demoMode);
    void SyncActive(const std::array<DeviceState, 3>& active);
    std::vector<HistoryItem> GetInactiveForKind(DeviceKind kind, const std::vector<DeviceEntry>& activeItems) const;
    bool RemoveEntry(DeviceKind kind, const std::string& key);
    bool ClearInactiveForKind(DeviceKind kind, const std::vector<DeviceEntry>& activeItems);

    static std::string FormatRelativeTime(int64_t lastSeenUnix);

private:
    void Save() const;
    static std::string GetStorageDir();
    static std::string GetStoragePath();

    std::vector<HistoryItem> items_;
    bool demoMode_ = false;
};