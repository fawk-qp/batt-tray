#include "icon.hpp"

#include <d2d1.h>
#include <dwrite.h>
#include <objbase.h>
#include <wincodec.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <vector>

namespace {

template <typename T>
struct ComPtr {
    T* p = nullptr;
    ComPtr() = default;
    ~ComPtr() { Reset(); }
    ComPtr(const ComPtr&) = delete;
    ComPtr& operator=(const ComPtr&) = delete;
    void Reset() {
        if (p) {
            p->Release();
            p = nullptr;
        }
    }
    T* get() const { return p; }
    T* operator->() const { return p; }
    T** put() { Reset(); return &p; }
    explicit operator bool() const { return p != nullptr; }
};

struct D2DFactories {
    ComPtr<ID2D1Factory> d2d;
    ComPtr<IDWriteFactory> dwrite;
    ComPtr<IWICImagingFactory> wic;
    bool initialized = false;

    bool Ensure() {
        if (initialized) return d2d && dwrite && wic;
        initialized = true;
        if (FAILED(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, d2d.put()))) return false;
        if (FAILED(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
                reinterpret_cast<IUnknown**>(dwrite.put())))) return false;
        if (FAILED(CoCreateInstance(CLSID_WICImagingFactory, nullptr, CLSCTX_INPROC_SERVER,
                __uuidof(IWICImagingFactory), reinterpret_cast<void**>(wic.put())))) return false;
        return true;
    }

    void Reset() {
        wic.Reset();
        dwrite.Reset();
        d2d.Reset();
        initialized = false;
    }
};

D2DFactories g_fx;

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

int GetTaskbarIconSize() {
    HWND taskbar = FindWindowW(L"Shell_TrayWnd", nullptr);
    UINT dpi = taskbar ? GetDpiForWindow(taskbar) : GetDpiForSystem();
    if (dpi == 0) dpi = 96u;
    return std::clamp(GetSystemMetricsForDpi(SM_CXSMICON, dpi), 16, 64);
}

inline D2D1_COLOR_F Rgba(uint8_t r, uint8_t g, uint8_t b, float a = 1.0f) {
    return D2D1::ColorF(r / 255.0f, g / 255.0f, b / 255.0f, a);
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

void ShutdownIconRenderer() {
    g_fx.Reset();
}

HICON CreateBatteryTrayIcon(wchar_t glyph, const DeviceState& state, bool lightTheme) {
    if (!g_fx.Ensure()) return nullptr;

    const int size = GetTaskbarIconSize();
    const float fSize = static_cast<float>(size);
    const float scale = fSize / 16.0f;

    ComPtr<IWICBitmap> wicBmp;
    if (FAILED(g_fx.wic->CreateBitmap(static_cast<UINT>(size), static_cast<UINT>(size),
            GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad, wicBmp.put()))) {
        return nullptr;
    }

    const D2D1_RENDER_TARGET_PROPERTIES rtProps = D2D1::RenderTargetProperties(
        D2D1_RENDER_TARGET_TYPE_SOFTWARE,
        D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED),
        96.0f, 96.0f);

    ComPtr<ID2D1RenderTarget> rt;
    if (FAILED(g_fx.d2d->CreateWicBitmapRenderTarget(wicBmp.get(), rtProps, rt.put()))) {
        return nullptr;
    }

    rt->SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
    rt->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);

    // Proportions aligned to the Windows 11 16-DIP tray icon grid
    const float barH = std::max(2.0f, std::round(2.0f * scale));
    const float bottomPad = (size >= 24) ? 1.0f : 0.0f;
    const float gap = std::max(1.0f, std::round(1.0f * scale));
    const float padX = std::max(1.0f, std::round(1.0f * scale));
    const float barY = fSize - bottomPad - barH;
    const float glyphBoxH = std::max(10.0f, barY - gap);
    const float fontPx = std::round(12.2f * scale);

    ComPtr<IDWriteTextFormat> fmt;
    if (FAILED(g_fx.dwrite->CreateTextFormat(
            PickIconFontName(), nullptr,
            DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL,
            fontPx, L"", fmt.put()))) {
        return nullptr;
    }
    fmt->SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
    fmt->SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);

    wchar_t text[2] = {glyph, L'\0'};
    ComPtr<IDWriteTextLayout> layout;
    if (FAILED(g_fx.dwrite->CreateTextLayout(text, 1, fmt.get(), fSize, glyphBoxH, layout.put()))) {
        return nullptr;
    }

    // Windows 11 Fluent icon colors
    const float glyphAlpha = state.online ? (lightTheme ? 0.92f : 1.0f) : 0.42f;
    const D2D1_COLOR_F glyphCol = lightTheme ? Rgba(0x1A, 0x1A, 0x1A, glyphAlpha)
                                             : Rgba(0xFF, 0xFF, 0xFF, glyphAlpha);

    ComPtr<ID2D1SolidColorBrush> glyphBrush;
    if (FAILED(rt->CreateSolidColorBrush(glyphCol, glyphBrush.put()))) return nullptr;

    rt->BeginDraw();
    rt->Clear(D2D1::ColorF(0.0f, 0.0f, 0.0f, 0.0f));
    rt->DrawTextLayout(D2D1::Point2F(0.0f, 0.0f), layout.get(), glyphBrush.get(), D2D1_DRAW_TEXT_OPTIONS_NONE);

    if (state.online && state.percent >= 0) {
        const float barW = std::max(4.0f, fSize - 2.0f * padX);
        const float radius = barH * 0.5f;

        const D2D1_COLOR_F trackCol = lightTheme ? Rgba(0x00, 0x00, 0x00, 0.18f)
                                                 : Rgba(0xFF, 0xFF, 0xFF, 0.24f);
        ComPtr<ID2D1SolidColorBrush> trackBrush;
        if (SUCCEEDED(rt->CreateSolidColorBrush(trackCol, trackBrush.put()))) {
            const D2D1_ROUNDED_RECT track = D2D1::RoundedRect(
                D2D1::RectF(padX, barY, padX + barW, barY + barH), radius, radius);
            rt->FillRoundedRectangle(track, trackBrush.get());
        }

        if (state.percent > 0) {
            D2D1_COLOR_F fillCol{};
            if (state.charging) {
                fillCol = lightTheme ? Rgba(0x00, 0x5F, 0xB8) : Rgba(0x60, 0xCD, 0xFF);
            } else if (state.percent <= 20) {
                fillCol = lightTheme ? Rgba(0xC4, 0x2B, 0x1C) : Rgba(0xFF, 0x99, 0xA4);
            } else if (state.percent <= 30) {
                fillCol = lightTheme ? Rgba(0x9D, 0x5D, 0x00) : Rgba(0xFC, 0xE1, 0x00);
            } else {
                fillCol = lightTheme ? Rgba(0x0F, 0x7B, 0x0F) : Rgba(0x6C, 0xCB, 0x5F);
            }

            const float fillW = std::clamp(barW * (state.percent / 100.0f), barH, barW);
            ComPtr<ID2D1SolidColorBrush> fillBrush;
            if (SUCCEEDED(rt->CreateSolidColorBrush(fillCol, fillBrush.put()))) {
                const D2D1_ROUNDED_RECT fill = D2D1::RoundedRect(
                    D2D1::RectF(padX, barY, padX + fillW, barY + barH), radius, radius);
                rt->FillRoundedRectangle(fill, fillBrush.get());
            }
        }
    }

    if (FAILED(rt->EndDraw())) return nullptr;

    HDC screenDc = GetDC(nullptr);
    if (!screenDc) return nullptr;

    BITMAPINFO bmi{};
    bmi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    bmi.bmiHeader.biWidth = size;
    bmi.bmiHeader.biHeight = -size; // top-down DIB
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    void* bits = nullptr;
    HBITMAP colorBmp = CreateDIBSection(screenDc, &bmi, DIB_RGB_COLORS, &bits, nullptr, 0);
    ReleaseDC(nullptr, screenDc);
    if (!colorBmp || !bits) {
        if (colorBmp) DeleteObject(colorBmp);
        return nullptr;
    }

    const UINT stride = static_cast<UINT>(size) * 4u;
    if (FAILED(wicBmp->CopyPixels(nullptr, stride, stride * static_cast<UINT>(size), static_cast<BYTE*>(bits)))) {
        DeleteObject(colorBmp);
        return nullptr;
    }

    // Explicitly zeroed WORD-aligned 1bpp mask bitmap
    const size_t maskStride = ((static_cast<size_t>(size) + 15u) / 16u) * 2u;
    std::vector<uint8_t> maskZero(maskStride * static_cast<size_t>(size), 0u);
    HBITMAP maskBmp = CreateBitmap(size, size, 1, 1, maskZero.data());

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