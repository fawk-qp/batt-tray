#pragma once

#include "device.hpp"

#include <array>
#include <cstdint>

std::array<DeviceState, 3> ScanAllDevices();
bool DisconnectBluetoothDevice(uint64_t bthAddr);