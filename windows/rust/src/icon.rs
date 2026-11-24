use crate::device::DeviceState;
use crate::win32::*;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::OnceLock;

unsafe extern "system" fn font_enum_proc(_: *const LOGFONTW, _: *const c_void, _: u32, lparam: LPARAM) -> i32 {
    *(lparam as *mut bool) = true;
    0
}

fn pick_icon_font() -> &'static str {
    static FONT: OnceLock<&'static str> = OnceLock::new();
    FONT.get_or_init(|| unsafe {
        let hdc = GetDC(null_mut());
        let mut found = false;
        if !hdc.is_null() {
            let mut lf = LOGFONTW {
                lfHeight: 0, lfWidth: 0, lfEscapement: 0, lfOrientation: 0, lfWeight: 0,
                lfItalic: 0, lfUnderline: 0, lfStrikeOut: 0, lfCharSet: DEFAULT_CHARSET,
                lfOutPrecision: 0, lfClipPrecision: 0, lfQuality: 0, lfPitchAndFamily: 0,
                lfFaceName: [0; 32],
            };
            for (dst, src) in lf.lfFaceName.iter_mut().zip("Segoe Fluent Icons".encode_utf16()) {
                *dst = src;
            }
            EnumFontFamiliesExW(hdc, &mut lf, font_enum_proc, &mut found as *mut bool as LPARAM, 0);
            ReleaseDC(null_mut(), hdc);
        }
        if found { "Segoe Fluent Icons" } else { "Segoe MDL2 Assets" }
    })
}

#[inline]
fn write_premul_bgra(px: &mut [u8], r: u8, g: u8, b: u8, a: u8) {
    let a32 = a as u32;
    px[0] = ((b as u32 * a32 + 127) / 255) as u8;
    px[1] = ((g as u32 * a32 + 127) / 255) as u8;
    px[2] = ((r as u32 * a32 + 127) / 255) as u8;
    px[3] = a;
}

pub fn is_taskbar_light_theme() -> bool {
    let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let value = to_wide("SystemUsesLightTheme");
    let mut data = 0u32;
    let mut cb = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut data as *mut u32 as *mut c_void,
            &mut cb,
        )
    };
    rc == ERROR_SUCCESS && data != 0
}

pub fn create_battery_tray_icon(glyph: u16, state: &DeviceState, light_theme: bool) -> HICON {
    unsafe {
        let size = GetSystemMetrics(SM_CXSMICON).clamp(16, 64);

        let screen_dc = GetDC(null_mut());
        if screen_dc.is_null() {
            return null_mut();
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        ReleaseDC(null_mut(), screen_dc);
        if mem_dc.is_null() {
            return null_mut();
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size, // top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [0],
        };

        let mut bits: *mut c_void = null_mut();
        let color_bmp = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if color_bmp.is_null() || bits.is_null() {
            if !color_bmp.is_null() {
                DeleteObject(color_bmp);
            }
            DeleteDC(mem_dc);
            return null_mut();
        }

        let total_bytes = (size as usize) * (size as usize) * 4;
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u8, total_bytes);
        pixels.fill(0);

        let old_bmp = SelectObject(mem_dc, color_bmp);
        let bar_h = (size / 10).max(2);
        let glyph_box_h = size - bar_h - 1;

        let mut lf = LOGFONTW {
            lfHeight: -((glyph_box_h * 92) / 100).max(10),
            lfWidth: 0, lfEscapement: 0, lfOrientation: 0, lfWeight: FW_NORMAL,
            lfItalic: 0, lfUnderline: 0, lfStrikeOut: 0, lfCharSet: DEFAULT_CHARSET,
            lfOutPrecision: 0, lfClipPrecision: 0, lfQuality: ANTIALIASED_QUALITY, lfPitchAndFamily: 0,
            lfFaceName: [0; 32],
        };
        for (dst, src) in lf.lfFaceName.iter_mut().zip(pick_icon_font().encode_utf16()) {
            *dst = src;
        }

        let font = CreateFontIndirectW(&lf);
        if !font.is_null() {
            let old_font = SelectObject(mem_dc, font);
            SetBkMode(mem_dc, TRANSPARENT);
            SetTextColor(mem_dc, 0x00FF_FFFF);
            let mut rc = RECT { left: 0, top: 0, right: size, bottom: glyph_box_h };
            let text = [glyph, 0u16];
            DrawTextW(mem_dc, text.as_ptr(), 1, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            SelectObject(mem_dc, old_font);
            DeleteObject(font);
        }

        GdiFlush();

        let fg: u8 = if light_theme { 0x1F } else { 0xFF };
        let opacity: u32 = if state.online { 255 } else { 115 };
        for px in pixels.chunks_exact_mut(4) {
            let cov = px[0].max(px[1]).max(px[2]);
            let alpha = ((cov as u32 * opacity + 127) / 255) as u8;
            write_premul_bgra(px, fg, fg, fg, alpha);
        }

        if state.online && state.percent >= 0 {
            let pad_x = (size / 12).max(1);
            let bar_w = (size - pad_x * 2).max(2);
            let min_fill = if state.percent > 0 { 1 } else { 0 };
            let filled_w = ((bar_w * state.percent + 50) / 100).clamp(min_fill, bar_w);

            let (bar_r, bar_g, bar_b) = if !state.charging && state.percent <= 20 {
                (0xE5, 0x39, 0x35)
            } else if state.charging {
                (0x38, 0xB6, 0xFF)
            } else {
                (0x2E, 0xCC, 0x71)
            };
            let trk: u8 = if light_theme { 0x20 } else { 0xFF };

            for y in (size - bar_h)..size {
                for x in pad_x..(pad_x + bar_w) {
                    let off = ((y as usize) * (size as usize) + (x as usize)) * 4;
                    let px = &mut pixels[off..off + 4];
                    if x < pad_x + filled_w {
                        write_premul_bgra(px, bar_r, bar_g, bar_b, 255);
                    } else {
                        write_premul_bgra(px, trk, trk, trk, 72);
                    }
                }
            }
        }

        SelectObject(mem_dc, old_bmp);
        DeleteDC(mem_dc);

        let mask_bmp = CreateBitmap(size, size, 1, 1, std::ptr::null());
        let mut icon = null_mut();
        if !mask_bmp.is_null() {
            let ii = ICONINFO {
                fIcon: 1,
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask_bmp,
                hbmColor: color_bmp,
            };
            icon = CreateIconIndirect(&ii);
            DeleteObject(mask_bmp);
        }
        DeleteObject(color_bmp);
        icon
    }
}