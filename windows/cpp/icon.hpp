#pragma once

#include "device.hpp"

bool IsTaskbarLightTheme();
HICON CreateBatteryTrayIcon(wchar_t glyph, const DeviceState& state, bool lightTheme);