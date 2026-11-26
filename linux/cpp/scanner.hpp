#pragma once

#include "device.hpp"

#include <dbus/dbus.h>

#include <array>

std::array<DeviceState, 3> ScanAllDevices(DBusConnection* sysBus, bool allowAirpodsBle = false);