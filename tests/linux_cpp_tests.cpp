#include "alert_tracker.hpp"
#include "device.hpp"

#include <cassert>
#include <string>

static void TestAlertTracker() {
    AlertTracker t;

    assert(t.Evaluate(0, "Mouse", 15, false, 100) == AlertLevel::Low15);
    assert(t.Evaluate(0, "Mouse", 14, false, 110) == AlertLevel::None);
    assert(t.Evaluate(0, "Mouse", 15, false, 120) == AlertLevel::None);
    assert(t.Evaluate(0, "Mouse", 14, false, 130) == AlertLevel::None);

    assert(t.Evaluate(0, "Mouse", 5, false, 140) == AlertLevel::Critical5);
    assert(t.Evaluate(0, "Mouse", 4, false, 150) == AlertLevel::None);

    assert(t.Evaluate(1, "Keyboard", 4, false, 160) == AlertLevel::Critical5);
    assert(t.Evaluate(1, "Keyboard", 10, false, 170) == AlertLevel::None);

    assert(t.Evaluate(0, "Mouse", 20, false, 180) == AlertLevel::None);
    assert(t.Evaluate(0, "Mouse", 15, false, 190) == AlertLevel::Low15);

    assert(t.Evaluate(0, "Mouse", 12, true, 200) == AlertLevel::None);
    assert(t.Evaluate(0, "Mouse", 12, false, 210) == AlertLevel::Low15);

    t.PruneOffline(210 + 1800);
    assert(t.Evaluate(0, "Mouse", 14, false, 2020) == AlertLevel::Low15);

    AlertTracker cap;
    for (int i = 0; i < 33; ++i) {
        assert(cap.Evaluate(0, "Dev" + std::to_string(i), 14, false, static_cast<uint64_t>(1000 + i)) == AlertLevel::Low15);
    }
    assert(cap.Evaluate(0, "Dev1", 14, false, 2000) == AlertLevel::None);
    assert(cap.Evaluate(0, "Dev0", 14, false, 2001) == AlertLevel::Low15);
}

static void TestSanitizeDeviceText() {
    // U+061C (\xD8\x9C), U+202E (\xE2\x80\xAE), U+200B (\xE2\x80\x8B), U+FEFF (\xEF\xBB\xBF)
    const std::string bidi = "\xD8\x9CMouse\xE2\x80\xAE \xE2\x80\x8BLogitech\xEF\xBB\xBF MX";
    assert(SanitizeDeviceText(bidi) == "Mouse Logitech MX");

    // Overlong UTF-8 (\xE0\x80\xAF) and UTF-16 surrogate (\xED\xA0\x80)
    const std::string malformed = "A\xE0\x80\xAF" "B\xED\xA0\x80" "C";
    assert(SanitizeDeviceText(malformed) == "ABC");

    const std::string validUtf8 = "Мышь Keychron K2";
    assert(SanitizeDeviceText(validUtf8) == "Мышь Keychron K2");
}

static void TestStripBatteryDetailSuffix() {
    assert(StripBatteryDetailSuffix("AirPods Pro (L:80% R:90% C:60%)") == "AirPods Pro");
    assert(StripBatteryDetailSuffix("AirPods Max (60%)") == "AirPods Max");
    assert(StripBatteryDetailSuffix("Headset (Stereo)") == "Headset (Stereo)");
}

int main() {
    TestAlertTracker();
    TestSanitizeDeviceText();
    TestStripBatteryDetailSuffix();
    return 0;
}
