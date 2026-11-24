use crate::device::DeviceState;
use crate::win32::*;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
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

fn get_taskbar_icon_size() -> i32 {
    unsafe {
        let cls = to_wide("Shell_TrayWnd");
        let tb = FindWindowW(cls.as_ptr(), null());
        let mut dpi = if !tb.is_null() { GetDpiForWindow(tb) } else { GetDpiForSystem() };
        if dpi == 0 {
            dpi = 96;
        }
        GetSystemMetricsForDpi(SM_CXSMICON, dpi).clamp(16, 64)
    }
}

#[inline]
fn premul_over(dst: &mut [u8], r: u8, g: u8, b: u8, a: f32) {
    let sa = a.clamp(0.0, 1.0);
    if sa <= 0.001 {
        return;
    }
    let sr = (r as f32 / 255.0) * sa;
    let sg = (g as f32 / 255.0) * sa;
    let sb = (b as f32 / 255.0) * sa;

    let inv = 1.0 - sa;
    let db = dst[0] as f32 / 255.0;
    let dg = dst[1] as f32 / 255.0;
    let dr = dst[2] as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;

    dst[0] = ((sb + db * inv).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    dst[1] = ((sg + dg * inv).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    dst[2] = ((sr + dr * inv).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    dst[3] = ((sa + da * inv).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
}

#[inline]
fn sd_rounded_box(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let cx = (x0 + x1) * 0.5;
    let cy = (y0 + y1) * 0.5;
    let hx = (x1 - x0) * 0.5;
    let hy = (y1 - y0) * 0.5;
    let rad = r.min(hx).min(hy);
    let qx = (px - cx).abs() - (hx - rad);
    let qy = (py - cy).abs() - (hy - rad);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - rad
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
        let size = get_taskbar_icon_size();
        let f_size = size as f32;
        let scale = f_size / 16.0;
        let ss: i32 = 4;
        let hi_size = size * ss;

        let bar_h = (2.0 * scale).round().max(2.0);
        let bottom_pad = if size >= 24 { 1.0 } else { 0.0 };
        let gap = (1.0 * scale).round().max(1.0);
        let pad_x = (1.0 * scale).round().max(1.0);
        let bar_y = f_size - bottom_pad - bar_h;
        let glyph_box_h = (bar_y - gap).max(10.0);
        let font_px = (12.2 * scale).round();

        let screen_dc = GetDC(null_mut());
        if screen_dc.is_null() {
            return null_mut();
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        if mem_dc.is_null() {
            ReleaseDC(null_mut(), screen_dc);
            return null_mut();
        }

        // 4x supersampled DIB for crisp glyph rasterization
        let hi_bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: hi_size,
                biHeight: -hi_size,
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

        let mut hi_bits: *mut c_void = null_mut();
        let hi_bmp = CreateDIBSection(mem_dc, &hi_bmi, DIB_RGB_COLORS, &mut hi_bits, null_mut(), 0);
        if hi_bmp.is_null() || hi_bits.is_null() {
            if !hi_bmp.is_null() { DeleteObject(hi_bmp); }
            DeleteDC(mem_dc);
            ReleaseDC(null_mut(), screen_dc);
            return null_mut();
        }

        let hi_len = (hi_size as usize) * (hi_size as usize) * 4;
        let hi_px = std::slice::from_raw_parts_mut(hi_bits as *mut u8, hi_len);
        hi_px.fill(0);

        let old_bmp = SelectObject(mem_dc, hi_bmp);
        let mut lf = LOGFONTW {
            lfHeight: -(font_px * ss as f32).round() as i32,
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
            let mut rc = RECT { left: 0, top: 0, right: hi_size, bottom: (glyph_box_h * ss as f32).round() as i32 };
            let text = [glyph, 0u16];
            DrawTextW(mem_dc, text.as_ptr(), 1, &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            SelectObject(mem_dc, old_font);
            DeleteObject(font);
        }
        GdiFlush();
        SelectObject(mem_dc, old_bmp);
        DeleteDC(mem_dc);

        // Final target DIB at exact taskbar DPI
        let out_bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
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

        let mut out_bits: *mut c_void = null_mut();
        let color_bmp = CreateDIBSection(screen_dc, &out_bmi, DIB_RGB_COLORS, &mut out_bits, null_mut(), 0);
        ReleaseDC(null_mut(), screen_dc);
        if color_bmp.is_null() || out_bits.is_null() {
            if !color_bmp.is_null() { DeleteObject(color_bmp); }
            DeleteObject(hi_bmp);
            return null_mut();
        }

        let out_px = std::slice::from_raw_parts_mut(out_bits as *mut u8, (size as usize) * (size as usize) * 4);
        out_px.fill(0);

        let fg: u8 = if light_theme { 0x1A } else { 0xFF };
        let glyph_alpha = if state.online {
            if light_theme { 0.92 } else { 1.0 }
        } else {
            0.42
        };

        // Gamma-correct 4x4 box downsample from supersampled DIB
        let hi_w = hi_size as usize;
        for y in 0..(size as usize) {
            for x in 0..(size as usize) {
                let mut sum_lin = 0.0f32;
                for sy in 0..(ss as usize) {
                    let row = (y * ss as usize + sy) * hi_w;
                    for sx in 0..(ss as usize) {
                        let off = (row + x * ss as usize + sx) * 4;
                        let c = hi_px[off].max(hi_px[off + 1]).max(hi_px[off + 2]) as f32 / 255.0;
                        sum_lin += c.powf(2.2);
                    }
                }
                let cov = (sum_lin / (ss * ss) as f32).powf(1.0 / 2.2);
                let a = (cov * 1.08).min(1.0) * glyph_alpha;
                let dst = &mut out_px[(y * size as usize + x) * 4..][..4];
                premul_over(dst, fg, fg, fg, a);
            }
        }
        DeleteObject(hi_bmp);

        // Analytical SDF anti-aliased Windows 11 pill indicator
        if state.online && state.percent >= 0 {
            let bar_w = (f_size - 2.0 * pad_x).max(4.0);
            let r = bar_h * 0.5;
            let (tr, tg, tb, ta) = if light_theme { (0x00, 0x00, 0x00, 0.18f32) } else { (0xFF, 0xFF, 0xFF, 0.24f32) };
            let (fr, fg_c, fb) = if state.charging {
                if light_theme { (0x00, 0x5F, 0xB8) } else { (0x60, 0xCD, 0xFF) }
            } else if state.percent <= 20 {
                if light_theme { (0xC4, 0x2B, 0x1C) } else { (0xFF, 0x99, 0xA4) }
            } else if state.percent <= 30 {
                if light_theme { (0x9D, 0x5D, 0x00) } else { (0xFC, 0xE1, 0x00) }
            } else if light_theme {
                (0x0F, 0x7B, 0x0F)
            } else {
                (0x6C, 0xCB, 0x5F)
            };
            let fill_w = if state.percent > 0 {
                (bar_w * (state.percent as f32 / 100.0)).clamp(bar_h, bar_w)
            } else {
                0.0
            };

            let y_min = ((bar_y - 1.0).floor().max(0.0)) as usize;
            let y_max = ((bar_y + bar_h + 1.0).ceil().min(f_size)) as usize;
            for y in y_min..y_max {
                let py = y as f32 + 0.5;
                for x in 0..(size as usize) {
                    let px = x as f32 + 0.5;
                    let dst = &mut out_px[(y * size as usize + x) * 4..][..4];
                    let d_track = sd_rounded_box(px, py, pad_x, bar_y, pad_x + bar_w, bar_y + bar_h, r);
                    let c_track = (0.5 - d_track).clamp(0.0, 1.0);
                    if c_track > 0.0 {
                        premul_over(dst, tr, tg, tb, c_track * ta);
                    }
                    if fill_w > 0.0 {
                        let d_fill = sd_rounded_box(px, py, pad_x, bar_y, pad_x + fill_w, bar_y + bar_h, r);
                        let c_fill = (0.5 - d_fill).clamp(0.0, 1.0);
                        if c_fill > 0.0 {
                            premul_over(dst, fr, fg_c, fb, c_fill);
                        }
                    }
                }
            }
        }

        let mask_stride = ((size as usize + 15) / 16) * 2;
        let mask_zero = vec![0u8; mask_stride * size as usize];
        let mask_bmp = CreateBitmap(size, size, 1, 1, mask_zero.as_ptr() as *const c_void);

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