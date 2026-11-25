#pragma once

#include "device.hpp"

bool IsTaskbarLightTheme();
int GetTaskbarIconSize();
HICON CreateBatteryTrayIcon(wchar_t glyph, const DeviceState& state, bool lightTheme);
void ShutdownIconRenderer();