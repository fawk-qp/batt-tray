#include "alert_tracker.hpp"
#include "device.hpp"
#include "tray.hpp"

#include <cassert>
#include <string>

static void TestAlertTracker() {
    AlertTracker t;

    assert(t.Evaluate(0, L"Mouse", 15, false, 100) == AlertLevel::Low15);
    assert(t.Evaluate(0, L"Mouse", 14, false, 110) == AlertLevel::None);
    assert(t.Evaluate(0, L"Mouse", 15, false, 120) == AlertLevel::None);
    assert(t.Evaluate(0, L"Mouse", 14, false, 130) == AlertLevel::None);

    // Jump from Low15 to <= 5 triggers Critical5 once
    assert(t.Evaluate(0, L"Mouse", 5, false, 140) == AlertLevel::Critical5);
    assert(t.Evaluate(0, L"Mouse", 4, false, 150) == AlertLevel::None);

    // Direct <= 5 without prior Low15 emits only Critical5
    assert(t.Evaluate(1, L"Keyboard", 4, false, 160) == AlertLevel::Critical5);
    assert(t.Evaluate(1, L"Keyboard", 10, false, 170) == AlertLevel::None);

    // Reset at >= 20%
    assert(t.Evaluate(0, L"Mouse", 20, false, 180) == AlertLevel::None);
    assert(t.Evaluate(0, L"Mouse", 15, false, 190) == AlertLevel::Low15);

    // Reset on charging
    assert(t.Evaluate(0, L"Mouse", 12, true, 200) == AlertLevel::None);
    assert(t.Evaluate(0, L"Mouse", 12, false, 210) == AlertLevel::Low15);

    // Offline reset after 1800 seconds
    t.PruneOffline(210 + 1800);
    assert(t.Evaluate(0, L"Mouse", 14, false, 2020) == AlertLevel::Low15);

    // Eviction when exceeding 32 entries (oldest by lastSeenSec is evicted)
    AlertTracker cap;
    for (int i = 0; i < 33; ++i) {
        assert(cap.Evaluate(0, L"Dev" + std::to_wstring(i), 14, false, static_cast<uint64_t>(1000 + i)) == AlertLevel::Low15);
    }
    // Dev0 was evicted, so it triggers again; Dev1 remains tracked
    assert(cap.Evaluate(0, L"Dev1", 14, false, 2000) == AlertLevel::None);
    assert(cap.Evaluate(0, L"Dev0", 14, false, 2001) == AlertLevel::Low15);
}

static void TestSanitizeDeviceText() {
    const std::wstring dirty = L"\u061CMouse\u202E \u200BLogitech\uFEFF MX";
    assert(SanitizeDeviceText(dirty) == L"Mouse Logitech MX");

    const std::wstring cyr = L"Мышь Keychron K2";
    assert(SanitizeDeviceText(cyr) == L"Мышь Keychron K2");
}

static void TestComputeSlotVisibility() {
    const bool allOff[3] = {false, false, false};
    const bool oneOn[3] = {true, false, false};

    auto v1 = ComputeSlotVisibility(allOff, false, 2);
    assert(v1.visible[0] && v1.visible[1] && v1.visible[2]);
    assert(!v1.anchorMode);

    auto v2 = ComputeSlotVisibility(oneOn, true, 2);
    assert(v2.visible[0] && !v2.visible[1] && !v2.visible[2]);
    assert(!v2.anchorMode);

    auto v3 = ComputeSlotVisibility(allOff, true, 2);
    assert(!v3.visible[0] && !v3.visible[1] && v3.visible[2]);
    assert(v3.anchorMode);
    assert(v3.anchorSlot == 2);

    auto v4 = ComputeSlotVisibility(allOff, true, 1);
    assert(!v4.visible[0] && v4.visible[1] && !v4.visible[2]);
    assert(v4.anchorMode);
    assert(v4.anchorSlot == 1);
}

int main() {
    TestAlertTracker();
    TestSanitizeDeviceText();
    TestComputeSlotVisibility();
    return 0;
}
