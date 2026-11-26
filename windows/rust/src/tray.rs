use crate::alert_tracker::{AlertLevel, AlertTracker};
use crate::device::{DeviceEntry, DeviceState, SLOT_SPECS};
use crate::history::DeviceHistory;
use crate::icon::{create_battery_tray_icon, get_taskbar_icon_size, is_taskbar_light_theme};
use crate::win32::*;
use std::ffi::c_void;
use std::ptr::{null, null_mut};

pub const WM_APP_TRAY: u32 = WM_APP + 1;
pub const WM_APP_SCAN_DONE: u32 = WM_APP + 2;

pub const IDM_HEADER: usize = 2000;
pub const IDM_REFRESH: usize = 2001;
pub const IDM_AUTOSTART: usize = 2002;
pub const IDM_EXIT: usize = 2003;
pub const IDM_HIDE_OFFLINE: usize = 2004;
pub const IDM_LOW_BATTERY_ALERT: usize = 2005;
pub const IDM_HIST_CLEAR: usize = 2010;
pub const IDM_HIST_DEL_BASE: usize = 2100;
pub const IDM_DISCONNECT_BASE: usize = 2200;

const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "BattTray";
const SETTINGS_SUBKEY: &str = "Software\\BattTray";
const HIDE_OFFLINE_VALUE: &str = "HideOffline";
const LOW_BATTERY_ALERT_VALUE: &str = "LowBatteryAlert";
const ANCHOR_TOOLTIP: &str = "BattTray \u{2014} нет подключённых устройств";

struct SlotVisibility {
    visible: [bool; 3],
    anchor_mode: bool,
    anchor_slot: usize,
}

fn compute_slot_visibility(online: [bool; 3], hide_offline: bool, last_anchor: usize) -> SlotVisibility {
    let mut v = SlotVisibility {
        visible: [true, true, true],
        anchor_mode: false,
        anchor_slot: if last_anchor < 3 { last_anchor } else { 2 },
    };
    if !hide_offline {
        return v;
    }
    let mut online_count = 0usize;
    let mut first_online = None;
    for i in 0..3 {
        v.visible[i] = online[i];
        if online[i] {
            online_count += 1;
            if first_online.is_none() {
                first_online = Some(i);
            }
        }
    }
    if online_count > 0 {
        if !online[v.anchor_slot] {
            if let Some(first) = first_online {
                v.anchor_slot = first;
            }
        }
        return v;
    }
    v.anchor_mode = true;
    v.visible = [false, false, false];
    v.visible[v.anchor_slot] = true;
    v
}

fn load_dword_setting(name: &str, default_val: u32) -> u32 {
    let subkey = to_wide(SETTINGS_SUBKEY);
    let val_name = to_wide(name);
    let mut val = 0u32;
    let mut ty = 0u32;
    let mut cb = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            val_name.as_ptr(),
            RRF_RT_REG_DWORD,
            &mut ty,
            &mut val as *mut u32 as *mut c_void,
            &mut cb,
        )
    };
    if rc == ERROR_SUCCESS { val } else { default_val }
}

fn save_dword_setting(name: &str, val: u32) {
    let subkey = to_wide(SETTINGS_SUBKEY);
    let val_name = to_wide(name);
    unsafe {
        let mut key: HKEY = null_mut();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            null(),
            0,
            KEY_SET_VALUE,
            null_mut(),
            &mut key,
            null_mut(),
        ) != ERROR_SUCCESS
        {
            return;
        }
        RegSetValueExW(
            key,
            val_name.as_ptr(),
            0,
            REG_DWORD,
            &val as *const u32 as *const u8,
            std::mem::size_of::<u32>() as u32,
        );
        RegCloseKey(key);
    }
}

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
    hide_offline: bool,
    low_battery_alert: bool,
    anchor_slot: usize,
    added_in_tray: [bool; 3],
    history: DeviceHistory,
    alert_tracker: AlertTracker,
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
        let hide_offline = load_dword_setting(HIDE_OFFLINE_VALUE, 0) != 0;
        let low_battery_alert = load_dword_setting(LOW_BATTERY_ALERT_VALUE, 1) != 0;
        let mut mgr = Self {
            hwnd,
            light_theme: is_taskbar_light_theme(),
            hide_offline,
            low_battery_alert,
            anchor_slot: 2,
            added_in_tray: [false; 3],
            history,
            alert_tracker: AlertTracker::new(),
            states: Default::default(),
            icons: [null_mut(); 3],
            cache: Default::default(),
            menu_slot: 0,
            menu_inactive_keys: Vec::new(),
            menu_disconnect_addrs: Vec::new(),
        };
        let icon_size = get_taskbar_icon_size();
        mgr.sync_visibility(icon_size);
        mgr
    }

    pub fn readd_icons(&mut self) {
        if self.hwnd.is_null() {
            return;
        }
        self.light_theme = is_taskbar_light_theme();
        let icon_size = get_taskbar_icon_size();
        let online = [self.states[0].online, self.states[1].online, self.states[2].online];
        let vis = compute_slot_visibility(online, self.hide_offline, self.anchor_slot);
        self.anchor_slot = vis.anchor_slot;
        for i in 0..3 {
            if !vis.visible[i] {
                continue;
            }
            let tip = if vis.anchor_mode && i == vis.anchor_slot {
                ANCHOR_TOOLTIP.to_string()
            } else {
                format_tooltip(SLOT_SPECS[i].label, &self.states[i])
            };
            self.apply_slot(i, true, icon_size, &tip);
        }
    }

    pub fn update(&mut self, states: [DeviceState; 3]) -> bool {
        self.states = states;
        self.history.sync_active(&self.states);
        self.sync_visibility(get_taskbar_icon_size())
    }

    pub fn evaluate_low_battery_alerts(&mut self, states: &[DeviceState; 3]) {
        let now_sec = unsafe { GetTickCount64() } / 1000;
        self.alert_tracker.prune_offline(now_sec);
        if !self.low_battery_alert {
            return;
        }
        for i in 0..3 {
            if !states[i].online {
                continue;
            }
            for d in &states[i].items {
                let dev_name = if d.name.is_empty() {
                    SLOT_SPECS[i].label
                } else {
                    &d.name
                };
                let lvl = self
                    .alert_tracker
                    .evaluate(i, dev_name, d.percent, d.charging, now_sec);
                if lvl == AlertLevel::None {
                    continue;
                }
                let critical = lvl == AlertLevel::Critical5;
                let title = if critical {
                    format!("Критический заряд: {}", dev_name)
                } else {
                    format!("Низкий заряд: {}", dev_name)
                };
                let body = format!("Осталось {}% заряда", d.percent);
                self.show_low_battery_toast(i, &title, &body, critical);
            }
        }
    }

    pub fn show_low_battery_toast(&self, slot: usize, title: &str, body: &str, critical: bool) {
        if slot >= 3 || self.hwnd.is_null() || !self.added_in_tray[slot] {
            return;
        }
        let mut nid = zero_nid(self.hwnd, SLOT_SPECS[slot].uid);
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = if critical { NIIF_ERROR } else { NIIF_WARNING };

        let wide_title: Vec<u16> = title.encode_utf16().take(63).collect();
        nid.szInfoTitle[..wide_title.len()].copy_from_slice(&wide_title);
        nid.szInfoTitle[wide_title.len()] = 0;

        let wide_body: Vec<u16> = body.encode_utf16().take(255).collect();
        nid.szInfo[..wide_body.len()].copy_from_slice(&wide_body);
        nid.szInfo[wide_body.len()] = 0;

        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &mut nid);
        }
    }

    pub fn refresh_theme(&mut self) -> bool {
        let light = is_taskbar_light_theme();
        if light == self.light_theme {
            return false;
        }
        self.light_theme = light;
        self.sync_visibility(get_taskbar_icon_size());
        true
    }

    pub fn toggle_hide_offline(&mut self) {
        self.hide_offline = !self.hide_offline;
        save_dword_setting(HIDE_OFFLINE_VALUE, u32::from(self.hide_offline));
        self.sync_visibility(get_taskbar_icon_size());
    }

    pub fn toggle_low_battery_alert(&mut self) {
        self.low_battery_alert = !self.low_battery_alert;
        save_dword_setting(LOW_BATTERY_ALERT_VALUE, u32::from(self.low_battery_alert));
        if !self.low_battery_alert {
            self.alert_tracker.clear();
        }
    }

    fn sync_visibility(&mut self, icon_size: i32) -> bool {
        let online = [self.states[0].online, self.states[1].online, self.states[2].online];
        let vis = compute_slot_visibility(online, self.hide_offline, self.anchor_slot);
        self.anchor_slot = vis.anchor_slot;

        let mut any_rendered = false;
        for i in 0..3 {
            if !vis.visible[i] {
                if self.added_in_tray[i] {
                    self.delete_slot(i);
                    any_rendered = true;
                }
                continue;
            }
            let tip = if vis.anchor_mode && i == vis.anchor_slot {
                ANCHOR_TOOLTIP.to_string()
            } else {
                format_tooltip(SLOT_SPECS[i].label, &self.states[i])
            };
            let c = &self.cache[i];
            let unchanged = self.added_in_tray[i]
                && c.valid
                && c.online == self.states[i].online
                && c.percent == self.states[i].percent
                && c.charging == self.states[i].charging
                && c.light_theme == self.light_theme
                && c.icon_size == icon_size
                && c.tip == tip;
            if unchanged {
                continue;
            }
            self.apply_slot(i, !self.added_in_tray[i], icon_size, &tip);
            any_rendered = true;
        }
        any_rendered
    }

    fn delete_slot(&mut self, idx: usize) {
        unsafe {
            if self.added_in_tray[idx] && !self.hwnd.is_null() {
                let mut nid = zero_nid(self.hwnd, SLOT_SPECS[idx].uid);
                Shell_NotifyIconW(NIM_DELETE, &mut nid);
            }
            self.added_in_tray[idx] = false;
            if !self.icons[idx].is_null() {
                DestroyIcon(self.icons[idx]);
                self.icons[idx] = null_mut();
            }
        }
        self.cache[idx] = SlotCache::default();
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
            let s_hide = to_wide("Скрывать отключённые");
            let s_alert = to_wide("Уведомлять при <15%");
            let s_auto = to_wide("Запускать с Windows");
            let s_exit = to_wide("Выход");

            AppendMenuW(menu, MF_STRING, IDM_REFRESH, s_refresh.as_ptr());
            let hide_flags = MF_STRING | if self.hide_offline { MF_CHECKED } else { MF_UNCHECKED };
            AppendMenuW(menu, hide_flags, IDM_HIDE_OFFLINE, s_hide.as_ptr());
            let alert_flags = MF_STRING | if self.low_battery_alert { MF_CHECKED } else { MF_UNCHECKED };
            AppendMenuW(menu, alert_flags, IDM_LOW_BATTERY_ALERT, s_alert.as_ptr());
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
        for i in 0..3 {
            self.delete_slot(i);
        }
        self.hwnd = null_mut();
    }

    fn apply_slot(&mut self, idx: usize, add: bool, icon_size: i32, tip: &str) {
        let spec = &SLOT_SPECS[idx];
        let next_icon = create_battery_tray_icon(spec.glyph, &self.states[idx], self.light_theme);
        if next_icon.is_null() {
            return;
        }

        let mut nid = zero_nid(self.hwnd, spec.uid);
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_APP_TRAY;
        nid.hIcon = next_icon;

        let wide_tip: Vec<u16> = tip.encode_utf16().take(127).collect();
        nid.szTip[..wide_tip.len()].copy_from_slice(&wide_tip);
        nid.szTip[wide_tip.len()] = 0;

        unsafe {
            let cmd = if add { NIM_ADD } else { NIM_MODIFY };
            let mut ok = Shell_NotifyIconW(cmd, &mut nid) != 0;
            if !ok && !add {
                ok = Shell_NotifyIconW(NIM_ADD, &mut nid) != 0;
            }
            if !ok {
                DestroyIcon(next_icon);
                self.added_in_tray[idx] = false;
                if !self.icons[idx].is_null() {
                    DestroyIcon(self.icons[idx]);
                    self.icons[idx] = null_mut();
                }
                self.cache[idx] = SlotCache::default();
                return;
            }
            self.added_in_tray[idx] = true;
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
            tip: tip.to_string(),
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
    if rc != ERROR_SUCCESS || cb < 2 {
        return false;
    }
    let mut buf = vec![0u16; (cb as usize / 2).max(1)];
    cb = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, subkey.as_ptr(), val.as_ptr(), RRF_RT_REG_SZ, &mut ty, buf.as_mut_ptr() as *mut c_void, &mut cb)
    };
    if rc != ERROR_SUCCESS {
        return false;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let raw = String::from_utf16_lossy(&buf[..end]);
    let stored = raw.trim().trim_matches('"').trim();
    match current_exe_path() {
        Some(exe) => stored.eq_ignore_ascii_case(exe.trim_matches('"').trim()),
        None => false,
    }
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