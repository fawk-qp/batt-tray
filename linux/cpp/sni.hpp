#pragma once

#include "device.hpp"

#include <dbus/dbus.h>

#include <array>
#include <cstdint>
#include <string>

struct ControlState {
    bool running = true;
    int64_t scanDueMs = 0; // 0 triggers an immediate scan on startup
    void ScheduleScan(int64_t delayMs);
};

int64_t NowMs();
void DrainDbusConn(DBusConnection* conn);

class SniManager {
public:
    ~SniManager();

    bool Init(bool hideOffline, ControlState* ctrl);
    void UpdateAll(const std::array<DeviceState, 3>& states);
    void RegisterAllWatchers();
    void DrainAll();
    void Shutdown();

    std::array<DBusConnection*, 3> Connections() const;

    struct Slot {
        const SlotSpec* spec = nullptr;
        SniManager* owner = nullptr;
        std::string busName;
        DBusConnection* conn = nullptr;
        std::string tipTitle;
        std::string tipDesc;
        std::string label;
        std::string menuStatus;
        std::string overlay;
        std::string status = "Active";
        uint32_t menuRevision = 1;
    };

    ControlState* Control() const { return ctrl_; }

private:
    void UpdateSlot(Slot& s, const DeviceState& d);

    std::array<Slot, 3> slots_{};
    bool hideOffline_ = false;
    ControlState* ctrl_ = nullptr;
};