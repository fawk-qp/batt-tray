#include "icon.hpp"

#include <algorithm>
#include <cstdint>
#include <cstring>

namespace {

int CALLBACK FontEnumProc(const LOGFONTW*, const TEXTMETRICW*, DWORD, LPARAM lParam) {
    *reinterpret_cast<bool*>(lParam) = true;
    return 0;
}

const wchar_t* PickIconFontName() {
    static const wchar_t* cached = nullptr;
    if (cached) return cached;

    HDC hdc = GetDC(nullptr);
    bool hasFluent = false;
    if (hdc) {
        LOGFONTW lf{};
        lf.lfCharSet = DEFAULT_CHARSET;
        wcscpy_s(lf.lfFaceName, L"Segoe Fluent Icons");
        EnumFontFamiliesExW(hdc, &lf, FontEnumProc, reinterpret_cast<LPARAM>(&hasFluent), 0);
        ReleaseDC(nullptr, hdc);
    }
    cached = hasFluent ? L"Segoe Fluent Icons" : L"Segoe MDL2 Assets";
    return cached;
}

inline void WritePremulBgra(uint8_t* px, uint8_t r, uint8_t g, uint8_t b, uint8_t a) {
    px[0] = static_cast<uint8_t>((static_cast<unsigned>(b) * a + 127u) / 255u);
    px[1] = static_cast<uint8_t>((static_cast<unsigned>(g) * a + 127u) / 255u);
    px[2] = static_cast<uint8_t>((static_cast<unsigned>(r) * a + 127u) / 255u);
    px[3] = a;
}

} // namespace

bool IsTaskbarLightTheme() {
    DWORD val = 0;
    DWORD cb = sizeof(val);
    const LONG rc = RegGetValueW(
        HKEY_CURRENT_USER,
        L"Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        L"SystemUsesLightTheme",
        RRF_RT_REG_DWORD,
        nullptr,
        &val,
        &cb);
    return (rc == ERROR_SUCCESS && val != 0);
}

HICON CreateBatteryTrayIcon(wchar_t glyph, const DeviceState& state, bool lightTheme) {
    const int size = std::clamp(GetSystemMetrics(SM_CXSMICON), 16, 64);

    HDC screenDc = GetDC(nullptr);
    if (!screenDc) return nullptr;
    HDC memDc = CreateCompatibleDC(screenDc);
    ReleaseDC(nullptr, screenDc);
    if (!memDc) return nullptr;

    BITMAPINFO bmi{};
    bmi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bmi.bmiHeader.biWidth = size;
    bmi.bmiHeader.biHeight = -size; // top-down DIB
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    void* bits = nullptr;
    HBITMAP colorBmp = CreateDIBSection(memDc, &bmi, DIB_RGB_COLORS, &bits, nullptr, 0);
    if (!colorBmp || !bits) {
        if (colorBmp) DeleteObject(colorBmp);
        DeleteDC(memDc);
        return nullptr;
    }

    auto* pixels = static_cast<uint8_t*>(bits);
    std::memset(pixels, 0, static_cast<size_t>(size) * static_cast<size_t>(size) * 4u);

    HGDIOBJ oldBmp = SelectObject(memDc, colorBmp);
    const int barH = std::max(2, size / 10);
    const int glyphBoxH = size - barH - 1;

    LOGFONTW lf{};
    lf.lfHeight = -std::max(10, (glyphBoxH * 92) / 100);
    lf.lfWeight = FW_NORMAL;
    lf.lfCharSet = DEFAULT_CHARSET;
    lf.lfQuality = ANTIALIASED_QUALITY;
    wcscpy_s(lf.lfFaceName, PickIconFontName());

    HFONT font = CreateFontIndirectW(&lf);
    if (font) {
        HGDIOBJ oldFont = SelectObject(memDc, font);
        SetBkMode(memDc, TRANSPARENT);
        SetTextColor(memDc, RGB(255, 255, 255));
        RECT rc{0, 0, size, glyphBoxH};
        wchar_t text[2] = {glyph, L'\0'};
        DrawTextW(memDc, text, 1, &rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(memDc, oldFont);
        DeleteObject(font);
    }

    // Flush queued GDI draw ops before reading grayscale coverage from the DIB buffer
    GdiFlush();

    const uint8_t fg = lightTheme ? 0x1Fu : 0xFFu;
    const unsigned opacity = state.online ? 255u : 115u;
    const size_t totalPixels = static_cast<size_t>(size) * static_cast<size_t>(size);
    for (size_t i = 0; i < totalPixels; ++i) {
        uint8_t* px = pixels + i * 4u;
        const uint8_t cov = std::max({px[0], px[1], px[2]});
        const uint8_t alpha = static_cast<uint8_t>((static_cast<unsigned>(cov) * opacity + 127u) / 255u);
        WritePremulBgra(px, fg, fg, fg, alpha);
    }

    if (state.online && state.percent >= 0) {
        const int padX = std::max(1, size / 12);
        const int barW = std::max(2, size - padX * 2);
        const int filledW = std::clamp((barW * state.percent + 50) / 100, (state.percent > 0 ? 1 : 0), barW);

        uint8_t barR = 0x2E, barG = 0xCC, barB = 0x71; // green
        if (!state.charging && state.percent <= 20) {
            barR = 0xE5; barG = 0x39; barB = 0x35; // red
        } else if (state.charging) {
            barR = 0x38; barG = 0xB6; barB = 0xFF; // accent blue while charging
        }
        const uint8_t trk = lightTheme ? 0x20u : 0xFFu;

        for (int y = size - barH; y < size; ++y) {
            for (int x = padX; x < padX + barW; ++x) {
                uint8_t* px = pixels + (static_cast<size_t>(y) * static_cast<size_t>(size) + static_cast<size_t>(x)) * 4u;
                if (x < padX + filledW) {
                    WritePremulBgra(px, barR, barG, barB, 255u);
                } else {
                    WritePremulBgra(px, trk, trk, trk, 72u);
                }
            }
        }
    }

    SelectObject(memDc, oldBmp);
    DeleteDC(memDc);

    HBITMAP maskBmp = CreateBitmap(size, size, 1, 1, nullptr);
    HICON icon = nullptr;
    if (maskBmp) {
        ICONINFO ii{};
        ii.fIcon = TRUE;
        ii.hbmColor = colorBmp;
        ii.hbmMask = maskBmp;
        icon = CreateIconIndirect(&ii);
        DeleteObject(maskBmp);
    }
    DeleteObject(colorBmp);
    return icon;
}