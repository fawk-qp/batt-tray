#pragma once

#include "device.hpp"

#include <dbus/dbus.h>

#include <array>
#include <cstdint>
#include <string>

struct ControlState {
    bool running = true;
    int64_t scanDueMs = -1;

    void ScheduleScan(int64_t delayMs);
};

class SniManager {
public:
    struct Slot {
        const SlotSpec* spec = nullptr;
        DBusConnection* conn = nullptr;
        std::string busName;
        std::string tipTitle;
        std::string tipDesc;
        std::string overlay;
        std::string status;
    };

    ~SniManager();

    bool Init(bool hideOffline, ControlState* ctrl);
    void UpdateAll(const std::array<DeviceState, 3>& states);
    void UpdateSlot(Slot& s, const DeviceState& d);
    void RegisterAllWatchers();
    void DrainAll();
    void Shutdown();
    std::array<DBusConnection*, 3> Connections() const;
    ControlState* Control() { return ctrl_; }

private:
    Slot slots_[3];
    bool hideOffline_ = false;
    ControlState* ctrl_ = nullptr;
};

int64_t NowMs();
void DrainDbusConn(DBusConnection* conn);
