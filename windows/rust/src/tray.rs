use crate::device::{DeviceEntry, DeviceState, SLOT_SPECS};
use crate::history::DeviceHistory;
use crate::icon::{create_battery_tray_icon, get_taskbar_icon_size, is_taskbar_light_theme};
use crate::win32::*;
use std::ptr::{null, null_mut};

pub const WM_APP_TRAY: u32 = WM_APP + 1;
pub const WM_APP_SCAN_DONE: u32 = WM_APP + 2;

pub const IDM_HEADER: usize = 2000;
pub const IDM_REFRESH: usize = 2001;
pub const IDM_AUTOSTART: usize = 2002;
pub const IDM_EXIT: usize = 2003;
pub const IDM_HIST_CLEAR: usize = 2010;
pub const IDM_HIST_DEL_BASE: usize = 2100;
pub const IDM_DISCONNECT_BASE: usize = 2200;

const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "BattTray";

#[derive(Clone, Default)]
struct SlotCache {
    valid: bool,
    online: bool,
    charging: bool,
    light_theme: bool,
    percent: i32,
    icon_size: i32,
    tip: String,
}

pub struct TrayManager {
    hwnd: HWND,
    light_theme: bool,
    history: DeviceHistory,
    states: [DeviceState; 3],
    icons: [HICON; 3],
    cache: [SlotCache; 3],
    menu_slot: usize,
    menu_inactive_keys: Vec<String>,
    menu_disconnect_addrs: Vec<u64>,
}

impl TrayManager {
    pub fn new(hwnd: HWND) -> Self {
        let mut history = DeviceHistory::default();
        history.load();
        let mut mgr = Self {
            hwnd,
            light_theme: is_taskbar_light_theme(),
            history,
            states: Default::default(),
            icons: [null_mut(); 3],
            cache: Default::default(),
            menu_slot: 0,
            menu_inactive_keys: Vec::new(),
            menu_disconnect_addrs: Vec::new(),
        };
        let icon_size = get_taskbar_icon_size();
        for i in 0..3 {
            mgr.apply_slot(i, true, icon_size);
        }
        mgr
    }

    pub fn readd_icons(&mut self) {
        if self.hwnd.is_null() {
            return;
        }
        self.light_theme = is_taskbar_light_theme();
        let icon_size = get_taskbar_icon_size();
        for i in 0..3 {
            self.apply_slot(i, true, icon_size);
        }
    }

    pub fn update(&mut self, states: [DeviceState; 3]) -> bool {
        self.states = states;
        self.history.sync_active(&self.states);

        let icon_size = get_taskbar_icon_size();
        let mut any_rendered = false;
        for i in 0..3 {
            let tip = format_tooltip(SLOT_SPECS[i].label, &self.states[i]);
            let c = &self.cache[i];
            let unchanged = c.valid
                && c.online == self.states[i].online
                && c.percent == self.states[i].percent
                && c.charging == self.states[i].charging
                && c.light_theme == self.light_theme
                && c.icon_size == icon_size
                && c.tip == tip;
            if unchanged {
                continue;
            }
            self.apply_slot(i, false, icon_size);
            any_rendered = true;
        }
        any_rendered
    }

    pub fn refresh_theme(&mut self) {
        let light = is_taskbar_light_theme();
        if light == self.light_theme {
            return;
        }
        self.light_theme = light;
        let icon_size = get_taskbar_icon_size();
        for i in 0..3 {
            self.apply_slot(i, false, icon_size);
        }
    }

    // Builds the popup HMENU without running TrackPopupMenu so RefCell borrows can be released before the modal loop.
    pub fn build_context_menu(&mut self, uid: u32) -> HMENU {
        self.menu_slot = SLOT_SPECS.iter().position(|s| s.uid == uid).unwrap_or(0);
        self.menu_inactive_keys.clear();
        self.menu_disconnect_addrs.clear();

        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return null_mut();
            }

            let spec = &SLOT_SPECS[self.menu_slot];
            let st = &self.states[self.menu_slot];
            let header = to_wide(&format_menu_header(spec.label, st));
            AppendMenuW(menu, MF_STRING | MF_GRAYED, IDM_HEADER, header.as_ptr());

            let active_bt: Vec<&DeviceEntry> = st.items.iter().filter(|d| d.bth_addr != 0).collect();
            if active_bt.len() == 1 {
                self.menu_disconnect_addrs.push(active_bt[0].bth_addr);
                let s_disc = to_wide("Отключить устройство");
                AppendMenuW(menu, MF_STRING, IDM_DISCONNECT_BASE, s_disc.as_ptr());
            } else if active_bt.len() > 1 {
                let disc_menu = CreatePopupMenu();
                if !disc_menu.is_null() {
                    for d in active_bt {
                        self.menu_disconnect_addrs.push(d.bth_addr);
                        let cmd_id = IDM_DISCONNECT_BASE + (self.menu_disconnect_addrs.len() - 1);
                        let title = if d.name.is_empty() {
                            spec.label.to_string()
                        } else {
                            truncate_chars(&d.name, 28)
                        };
                        let prefix = if d.active_audio { "\u{25B6} " } else { "\u{2022} " };
                        let row = to_wide(&format!("{}{} — {}%", prefix, title, d.percent));
                        AppendMenuW(disc_menu, MF_STRING, cmd_id, row.as_ptr());
                    }
                    let s_disc = to_wide("Отключить устройство");
                    AppendMenuW(menu, MF_POPUP, disc_menu as usize, s_disc.as_ptr());
                }
            }

            AppendMenuW(menu, MF_SEPARATOR, 0, null());

            let inactive = self.history.get_inactive_for_kind(spec.kind, &st.items);
            if !inactive.is_empty() {
                let hist_menu = CreatePopupMenu();
                if !hist_menu.is_null() {
                    let s_del = to_wide("Удалить из истории");
                    for item in inactive.iter().take(10) {
                        let item_sub = CreatePopupMenu();
                        if item_sub.is_null() {
                            continue;
                        }
                        self.menu_inactive_keys.push(item.key.clone());
                        let del_id = IDM_HIST_DEL_BASE + (self.menu_inactive_keys.len() - 1);
                        AppendMenuW(item_sub, MF_STRING, del_id, s_del.as_ptr());

                        let d_name = if item.name.is_empty() {
                            spec.label.to_string()
                        } else {
                            truncate_chars(&item.name, 28)
                        };
                        let label = to_wide(&format!(
                            "\u{25CB} {} — {}% ({})",
                            d_name,
                            item.percent,
                            DeviceHistory::format_relative_time(item.last_seen_unix)
                        ));
                        AppendMenuW(hist_menu, MF_POPUP, item_sub as usize, label.as_ptr());
                    }
                    AppendMenuW(hist_menu, MF_SEPARATOR, 0, null());
                    let s_clear = to_wide("Очистить историю");
                    AppendMenuW(hist_menu, MF_STRING, IDM_HIST_CLEAR, s_clear.as_ptr());

                    let s_hist = to_wide("История (неактивные)");
                    AppendMenuW(menu, MF_POPUP, hist_menu as usize, s_hist.as_ptr());
                    AppendMenuW(menu, MF_SEPARATOR, 0, null());
                }
            }

            let s_refresh = to_wide("Обновить сейчас");
            let s_auto = to_wide("Запускать с Windows");
            let s_exit = to_wide("Выход");

            AppendMenuW(menu, MF_STRING, IDM_REFRESH, s_refresh.as_ptr());
            let auto_flags = MF_STRING | if is_autostart_enabled() { MF_CHECKED } else { MF_UNCHECKED };
            AppendMenuW(menu, auto_flags, IDM_AUTOSTART, s_auto.as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            AppendMenuW(menu, MF_STRING, IDM_EXIT, s_exit.as_ptr());

            menu
        }
    }

    pub fn track_and_destroy_menu(hwnd: HWND, menu: HMENU) -> usize {
        if menu.is_null() {
            return 0;
        }
        unsafe {
            let mut pt = POINT::default();
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RETURNCMD,
                pt.x,
                pt.y,
                0,
                hwnd,
                null(),
            );
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);
            if cmd > 0 { cmd as usize } else { 0 }
        }
    }

    pub fn handle_menu_command(&mut self, cmd_id: usize) {
        let kind = SLOT_SPECS[self.menu_slot].kind;
        if cmd_id == IDM_HIST_CLEAR {
            self.history.clear_inactive_for_kind(kind, &self.states[self.menu_slot].items);
            self.menu_inactive_keys.clear();
            return;
        }
        if (IDM_HIST_DEL_BASE..IDM_DISCONNECT_BASE).contains(&cmd_id) {
            let idx = cmd_id - IDM_HIST_DEL_BASE;
            if let Some(key) = self.menu_inactive_keys.get(idx).cloned() {
                self.history.remove_entry(kind, &key);
            }
            self.menu_inactive_keys.clear();
        }
    }

    pub fn take_disconnect_addr(&mut self, cmd_id: usize) -> Option<u64> {
        if cmd_id < IDM_DISCONNECT_BASE {
            return None;
        }
        let idx = cmd_id - IDM_DISCONNECT_BASE;
        let addr = self.menu_disconnect_addrs.get(idx).copied().filter(|&a| a != 0);
        self.menu_disconnect_addrs.clear();
        addr
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

    fn apply_slot(&mut self, idx: usize, add: bool, icon_size: i32) {
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
        self.cache[idx] = SlotCache {
            valid: true,
            online: self.states[idx].online,
            charging: self.states[idx].charging,
            light_theme: self.light_theme,
            percent: self.states[idx].percent,
            icon_size,
            tip,
        };
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

fn truncate_chars(s: &str, max_chars: usize) -> String {
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

fn format_menu_header(label: &str, st: &DeviceState) -> String {
    if !st.online || st.items.is_empty() {
        return format!("{}: не подключено", label);
    }
    let d = &st.items[0];
    let title = if d.name.is_empty() { label.to_string() } else { truncate_chars(&d.name, 34) };
    let prefix = if st.items.len() > 1 { "\u{25B6} " } else { "" };
    let ch = if d.charging { " \u{26A1}" } else { "" };
    let extra = if st.items.len() > 1 { format!(" (+{})", st.items.len() - 1) } else { String::new() };
    format!("{}{} — {}%{}{}", prefix, title, d.percent, ch, extra)
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