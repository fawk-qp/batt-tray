#pragma once

#include <cstdint>
#include <string>
#include <vector>

enum class AlertLevel : uint8_t { None = 0, Low15 = 1, Critical5 = 2 };

class AlertTracker {
public:
    void Clear() { entries_.clear(); }

    void PruneOffline(uint64_t nowSec) {
        for (auto it = entries_.begin(); it != entries_.end();) {
            if (nowSec > it->lastSeenSec && (nowSec - it->lastSeenSec) >= kOfflineResetSec) {
                it = entries_.erase(it);
            } else {
                ++it;
            }
        }
    }

    AlertLevel Evaluate(int slot, const std::wstring& name, int percent, bool charging, uint64_t nowSec) {
        if (percent < 0 || percent > 100 || name.empty()) return AlertLevel::None;

        Entry* entry = FindOrCreate(slot, name, nowSec);
        entry->lastSeenSec = nowSec;

        if (charging || percent >= kResetThreshold) {
            entry->level = AlertLevel::None;
            return AlertLevel::None;
        }

        if (percent <= kCriticalThreshold && entry->level < AlertLevel::Critical5) {
            entry->level = AlertLevel::Critical5;
            return AlertLevel::Critical5;
        }
        if (percent <= kLowThreshold && entry->level < AlertLevel::Low15) {
            entry->level = AlertLevel::Low15;
            return AlertLevel::Low15;
        }
        return AlertLevel::None;
    }

private:
    struct Entry {
        int slot = 0;
        std::wstring name;
        AlertLevel level = AlertLevel::None;
        uint64_t lastSeenSec = 0;
    };

    Entry* FindOrCreate(int slot, const std::wstring& name, uint64_t nowSec) {
        for (auto& e : entries_) {
            if (e.slot == slot && e.name == name) return &e;
        }
        if (entries_.size() >= kMaxEntries) {
            auto oldest = entries_.begin();
            for (auto it = entries_.begin(); it != entries_.end(); ++it) {
                if (it->lastSeenSec < oldest->lastSeenSec) oldest = it;
            }
            *oldest = Entry{slot, name, AlertLevel::None, nowSec};
            return &(*oldest);
        }
        entries_.push_back(Entry{slot, name, AlertLevel::None, nowSec});
        return &entries_.back();
    }

    static constexpr int kLowThreshold = 15;
    static constexpr int kCriticalThreshold = 5;
    static constexpr int kResetThreshold = 20;
    static constexpr uint64_t kOfflineResetSec = 1800;
    static constexpr size_t kMaxEntries = 32;

    std::vector<Entry> entries_;
};
