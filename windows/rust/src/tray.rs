use crate::device::{DeviceState, SLOT_SPECS};
use crate::icon::{create_battery_tray_icon, is_taskbar_light_theme};
use crate::win32::*;
use std::ptr::{null, null_mut};

pub const WM_APP_TRAY: u32 = WM_APP + 1;
pub const WM_APP_SCAN_DONE: u32 = WM_APP + 2;

pub const IDM_REFRESH: usize = 2001;
pub const IDM_AUTOSTART: usize = 2002;
pub const IDM_EXIT: usize = 2003;

const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "BattTray";

pub struct TrayManager {
    hwnd: HWND,
    light_theme: bool,
    states: [DeviceState; 3],
    icons: [HICON; 3],
}

impl TrayManager {
    pub fn new(hwnd: HWND) -> Self {
        let mut mgr = Self {
            hwnd,
            light_theme: is_taskbar_light_theme(),
            states: Default::default(),
            icons: [null_mut(); 3],
        };
        for i in 0..3 {
            mgr.apply_slot(i, true);
        }
        mgr
    }

    pub fn readd_icons(&mut self) {
        if self.hwnd.is_null() {
            return;
        }
        self.light_theme = is_taskbar_light_theme();
        for i in 0..3 {
            self.apply_slot(i, true);
        }
    }

    pub fn update(&mut self, states: [DeviceState; 3]) {
        self.states = states;
        for i in 0..3 {
            self.apply_slot(i, false);
        }
    }

    pub fn refresh_theme(&mut self) {
        let light = is_taskbar_light_theme();
        if light == self.light_theme {
            return;
        }
        self.light_theme = light;
        for i in 0..3 {
            self.apply_slot(i, false);
        }
    }

    pub fn shutdown(&mut self) {
        unsafe {
            for i in 0..3 {
                if !self.hwnd.is_null() {
                    let mut nid = zero_nid(self.hwnd, SLOT_SPECS[i].uid);
                    Shell_NotifyIconW(NIM_DELETE, &mut nid);
                }
                if !self.icons[i].is_null() {
                    DestroyIcon(self.icons[i]);
                    self.icons[i] = null_mut();
                }
            }
        }
        self.hwnd = null_mut();
    }

    fn apply_slot(&mut self, idx: usize, add: bool) {
        let spec = &SLOT_SPECS[idx];
        let next_icon = create_battery_tray_icon(spec.glyph, &self.states[idx], self.light_theme);
        if next_icon.is_null() {
            return;
        }

        let tip = format_tooltip(spec.label, &self.states[idx]);
        let mut nid = zero_nid(self.hwnd, spec.uid);
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_APP_TRAY;
        nid.hIcon = next_icon;

        let wide_tip: Vec<u16> = tip.encode_utf16().take(127).collect();
        nid.szTip[..wide_tip.len()].copy_from_slice(&wide_tip);
        nid.szTip[wide_tip.len()] = 0;

        unsafe {
            let cmd = if add { NIM_ADD } else { NIM_MODIFY };
            if Shell_NotifyIconW(cmd, &mut nid) == 0 && !add {
                Shell_NotifyIconW(NIM_ADD, &mut nid);
            }
            if !self.icons[idx].is_null() {
                DestroyIcon(self.icons[idx]);
            }
        }
        self.icons[idx] = next_icon;
    }
}

impl Drop for TrayManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn zero_nid(hwnd: HWND, uid: u32) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: uid,
        uFlags: 0,
        uCallbackMessage: 0,
        hIcon: null_mut(),
        szTip: [0; 128],
        dwState: 0,
        dwStateMask: 0,
        szInfo: [0; 256],
        uTimeoutOrVersion: 0,
        szInfoTitle: [0; 64],
        dwInfoFlags: 0,
        guidItem: GUID::default(),
        hBalloonIcon: null_mut(),
    }
}

ffn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

fn format_tooltip(label: &str, st: &DeviceState) -> String {
    if !st.online || st.items.is_empty() {
        return format!("{}: не подключено", label);
    }
    if st.items.len() == 1 {
        let d = &st.items[0];
        let ch = if d.charging { " (зарядка)" } else { "" };
        return if d.name.is_empty() {
            format!("{}: {}%{}", label, d.percent, ch)
        } else {
            format!("{}: {}%{}\n{}", label, d.percent, ch, d.name)
        };
    }

    let name_cap = if st.items.len() >= 3 { 20 } else { 28 };
    let mut tip = String::new();
    for d in st.items.iter().take(4) {
        let prefix = if d.active_audio { "\u{25B6} " } else { "\u{2022} " };
        let title = if d.name.is_empty() { label.to_string() } else { truncate_chars(&d.name, name_cap) };
        let ch = if d.charging { " \u{26A1}" } else { "" };
        let line = format!("{}{}: {}%{}", prefix, title, d.percent, ch);
        let next_utf16 = tip.encode_utf16().count() + usize::from(!tip.is_empty()) + line.encode_utf16().count();
        if !tip.is_empty() && next_utf16 > 126 {
            break;
        }
        if !tip.is_empty() {
            tip.push('\n');
        }
        tip.push_str(&line);
    }
    tip
}

pub fn show_context_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return;
        }
        let s_refresh = to_wide("Обновить сейчас");
        let s_auto = to_wide("Запускать с Windows");
        let s_exit = to_wide("Выход");

        AppendMenuW(menu, MF_STRING, IDM_REFRESH, s_refresh.as_ptr());
        let auto_flags = MF_STRING | if is_autostart_enabled() { MF_CHECKED } else { MF_UNCHECKED };
        AppendMenuW(menu, auto_flags, IDM_AUTOSTART, s_auto.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null());
        AppendMenuW(menu, MF_STRING, IDM_EXIT, s_exit.as_ptr());

        let mut pt = POINT::default();
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN, pt.x, pt.y, 0, hwnd, null());
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }
}

pub fn is_autostart_enabled() -> bool {
    let subkey = to_wide(RUN_SUBKEY);
    let val = to_wide(RUN_VALUE);
    let mut ty = 0u32;
    let mut cb = 0u32;
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, subkey.as_ptr(), val.as_ptr(), RRF_RT_REG_SZ, &mut ty, null_mut(), &mut cb)
    };
    rc == ERROR_SUCCESS && cb > 2
}

pub fn toggle_autostart() {
    let subkey = to_wide(RUN_SUBKEY);
    let val = to_wide(RUN_VALUE);
    unsafe {
        let mut key: HKEY = null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_SET_VALUE, &mut key) != ERROR_SUCCESS {
            return;
        }
        if is_autostart_enabled() {
            RegDeleteValueW(key, val.as_ptr());
        } else if let Some(exe) = current_exe_path() {
            let quoted = to_wide(&format!("\"{}\"", exe));
            let cb = (quoted.len() * 2) as u32;
            RegSetValueExW(key, val.as_ptr(), 0, REG_SZ, quoted.as_ptr() as *const u8, cb);
        }
        RegCloseKey(key);
    }
}

fn current_exe_path() -> Option<String> {
    let mut buf = vec![0u16; 260];
    loop {
        let len = unsafe { GetModuleFileNameW(null_mut(), buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if len == 0 {
            return None;
        }
        if len < buf.len() - 1 {
            return Some(String::from_utf16_lossy(&buf[..len]));
        }
        if buf.len() >= 32768 {
            return None;
        }
        buf.resize(buf.len() * 2, 0);
    }
}