#pragma once

#include "device.hpp"
#include "history.hpp"

#include <dbus/dbus.h>

#include <array>
#include <cstdint>
#include <string>
#include <vector>

struct ControlState {
    bool running = true;
    int64_t scanDueMs = 0; // 0 triggers an immediate scan on startup
    void ScheduleScan(int64_t delayMs);
};

int64_t NowMs();
void DrainDbusConn(DBusConnection* conn);

struct InactiveMenuRow {
    std::string key;
    std::string label;
    bool operator==(const InactiveMenuRow& other) const {
        return key == other.key && label == other.label;
    }
};

class SniManager {
public:
    ~SniManager();

    bool Init(bool hideOffline, bool demoMode, ControlState* ctrl);
    void UpdateAll(const std::array<DeviceState, 3>& states);
    void HandleHistoryAction(size_t slotIdx, dbus_int32_t itemId);
    void RegisterAllWatchers();
    void DrainAll();
    void Shutdown();

    std::array<DBusConnection*, 3> Connections() const;

    struct Slot {
        size_t index = 0;
        const SlotSpec* spec = nullptr;
        SniManager* owner = nullptr;
        std::string busName;
        DBusConnection* conn = nullptr;
        std::string tipTitle;
        std::string tipDesc;
        std::string label;
        std::string menuStatus;
        std::vector<InactiveMenuRow> inactiveMenu;
        std::string overlay;
        std::string status = "Active";
        uint32_t menuRevision = 1;
    };

    ControlState* Control() const { return ctrl_; }

private:
    std::vector<InactiveMenuRow> BuildInactiveMenu(size_t slotIdx) const;
    void UpdateSlot(Slot& s, const DeviceState& d);

    std::array<Slot, 3> slots_{};
    std::array<DeviceState, 3> states_{};
    DeviceHistory history_;
    bool hideOffline_ = false;
    ControlState* ctrl_ = nullptr;
};