#pragma once

#include "device.hpp"

#include <array>

void ScanVendorHidDevices(std::array<DeviceState, 3>& out);
