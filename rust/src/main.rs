#![windows_subsystem = "windows"]

use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

type Handle = isize;
type Hwnd = isize;
type Hdc = isize;
type Hicon = isize;
type Hbitmap = isize;
type Hfont = isize;
type Hmenu = isize;
type Hkey = isize;
type Hmodule = isize;

const WM_DESTROY: u32 = 0x0002;
const WM_SETTINGCHANGE: u32 = 0x001A;
const WM_COMMAND: u32 = 0x0111;
const WM_TIMER: u32 = 0x0113;
const WM_DEVICECHANGE: u32 = 0x0219;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_CONTEXTMENU: u32 = 0x007B;
const WM_APP: u32 = 0x8000;
const WM_TRAY: u32 = WM_APP + 1;
const WM_UPDATE_UI: u32 = WM_APP + 2;
const SM_CXSMICON: i32 = 49;
const BLACK_BRUSH: i32 = 4;
const TRANSPARENT: i32 = 1;
const DRAW_TEXT_FLAGS: u32 = 0x0825;
const ERROR_ALREADY_EXISTS: u32 = 183;
const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;

const UID_MOUSE: u32 = 1;
const UID_HEADPHONES: u32 = 2;
const TIMER_DEBOUNCE: usize = 101;
const DBT_DEVNODES_CHANGED: usize = 0x0007;
const POLL_INTERVAL_MS: u32 = 60_000;

const DIGCF_PRESENT: u32 = 0x0000_0002;
const DIGCF_ALLCLASSES: u32 = 0x0000_0004;
const DIGCF_DEVICEINTERFACE: u32 = 0x0000_0010;
const INVALID_HANDLE_VALUE: Handle = -1;

const NIF_MESSAGE: u32 = 0x0000_0001;
const NIF_ICON: u32 = 0x0000_0002;
const NIF_TIP: u32 = 0x0000_0004;
const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;

const HKEY_CURRENT_USER: Hkey = 0x8000_0001u32 as i32 as isize;
const KEY_QUERY_VALUE: u32 = 0x0001;
const KEY_SET_VALUE: u32 = 0x0002;
const REG_SZ: u32 = 1;

const MF_STRING: u32 = 0x0000;
const MF_UNCHECKED: u32 = 0x0000;
const MF_CHECKED: u32 = 0x0008;
const MF_SEPARATOR: u32 = 0x0800;
const TPM_RIGHTBUTTON: u32 = 0x0002;

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DevPropKey {
    fmtid: Guid,
    pid: u32,
}

#[repr(C)]
struct SpDevinfoData {
    cb_size: u32,
    class_guid: Guid,
    dev_inst: u32,
    reserved: usize,
}

#[repr(C)]
struct SpDeviceInterfaceData {
    cb_size: u32,
    interface_class_guid: Guid,
    flags: u32,
    reserved: usize,
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: Point,
}

#[repr(C)]
struct WndClassW {
    style: u32,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: Hmodule,
    h_icon: Hicon,
    h_cursor: isize,
    hbr_background: isize,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
}

#[repr(C)]
struct NotifyIconDataW {
    cb_size: u32,
    h_wnd: Hwnd,
    u_id: u32,
    u_flags: u32,
    u_callback_message: u32,
    h_icon: Hicon,
    sz_tip: [u16; 128],
    dw_state: u32,
    dw_state_mask: u32,
    sz_info: [u16; 256],
    u_timeout_or_version: u32,
    sz_info_title: [u16; 64],
    dw_info_flags: u32,
    guid_item: Guid,
    h_balloon_icon: Hicon,
}

#[repr(C)]
struct BitmapInfoHeader {
    bi_size: u32,
    bi_width: i32,
    bi_height: i32,
    bi_planes: u16,
    bi_bit_count: u16,
    bi_compression: u32,
    bi_size_image: u32,
    bi_x_pels_per_meter: i32,
    bi_y_pels_per_meter: i32,
    bi_clr_used: u32,
    bi_clr_important: u32,
}

#[repr(C)]
struct BitmapInfo {
    bmi_header: BitmapInfoHeader,
    bmi_colors: [u32; 1],
}

#[repr(C)]
struct IconInfo {
    f_icon: i32,
    x_hotspot: u32,
    y_hotspot: u32,
    hbm_mask: Hbitmap,
    hbm_color: Hbitmap,
}

#[repr(C)]
#[derive(Clone, Copy)]
union BthLeUuidVal {
    short_uuid: u16,
    long_uuid: Guid,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BthLeUuid {
    is_short_uuid: u8,
    value: BthLeUuidVal,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BthLeGattCharacteristic {
    service_handle: u16,
    characteristic_uuid: BthLeUuid,
    attribute_handle: u16,
    characteristic_value_handle: u16,
    is_broadcastable: u8,
    is_readable: u8,
    is_writable: u8,
    is_writable_without_response: u8,
    is_signed_writable: u8,
    is_notifiable: u8,
    is_indicatable: u8,
    has_extended_properties: u8,
}

#[repr(C)]
struct BthLeGattCharacteristicValue {
    data_size: u32,
    data: [u8; 1],
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(attr: *const c_void, initial_owner: i32, name: *const u16) -> Handle;
    fn GetLastError() -> u32;
    fn ReleaseMutex(h: Handle) -> i32;
    fn CreateEventW(attr: *const c_void, manual_reset: i32, initial: i32, name: *const u16) -> Handle;
    fn SetEvent(h: Handle) -> i32;
    fn WaitForMultipleObjects(count: u32, handles: *const Handle, wait_all: i32, ms: u32) -> u32;
    fn CloseHandle(h: Handle) -> i32;
    fn GetModuleHandleW(name: *const u16) -> Hmodule;
    fn GetModuleFileNameW(h: Hmodule, buf: *mut u16, size: u32) -> u32;
    fn GetWindowsDirectoryW(buf: *mut u16, size: u32) -> u32;
    fn LoadLibraryExW(name: *const u16, file: Handle, flags: u32) -> Hmodule;
    fn GetProcAddress(h: Hmodule, name: *const u8) -> *const c_void;
    fn FreeLibrary(h: Hmodule) -> i32;
    fn CreateFileW(
        name: *const u16, access: u32, share: u32, sec: *const c_void,
        disp: u32, flags: u32, tmpl: Handle,
    ) -> Handle;
}

#[link(name = "user32")]
extern "system" {
    fn GetSystemMetrics(idx: i32) -> i32;
    fn GetDC(hwnd: Hwnd) -> Hdc;
    fn ReleaseDC(hwnd: Hwnd, hdc: Hdc) -> i32;
    fn FillRect(hdc: Hdc, rc: *const Rect, hbr: isize) -> i32;
    fn DrawTextW(hdc: Hdc, text: *const u16, count: i32, rc: *mut Rect, fmt: u32) -> i32;
    fn CreateIconIndirect(info: *const IconInfo) -> Hicon;
    fn DestroyIcon(icon: Hicon) -> i32;
    fn RegisterWindowMessageW(msg: *const u16) -> u32;
    fn RegisterClassW(wc: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex: u32, cls: *const u16, title: *const u16, style: u32,
        x: i32, y: i32, w: i32, h: i32, parent: Hwnd, menu: Hmenu, inst: Hmodule, param: *const c_void,
    ) -> Hwnd;
    fn DefWindowProcW(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize;
    fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn PostMessageW(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> i32;
    fn PostQuitMessage(code: i32);
    fn DestroyWindow(hwnd: Hwnd) -> i32;
    fn CreatePopupMenu() -> Hmenu;
    fn AppendMenuW(menu: Hmenu, flags: u32, id: usize, item: *const u16) -> i32;
    fn GetCursorPos(pt: *mut Point) -> i32;
    fn SetForegroundWindow(hwnd: Hwnd) -> i32;
    fn TrackPopupMenu(menu: Hmenu, flags: u32, x: i32, y: i32, res: i32, hwnd: Hwnd, rc: *const Rect) -> i32;
    fn DestroyMenu(menu: Hmenu) -> i32;
    fn SetTimer(hwnd: Hwnd, id: usize, elapse: u32, func: *const c_void) -> usize;
    fn KillTimer(hwnd: Hwnd, id: usize) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateDIBSection(hdc: Hdc, pbmi: *const BitmapInfo, usage: u32, bits: *mut *mut c_void, sec: Handle, off: u32) -> Hbitmap;
    fn CreateCompatibleDC(hdc: Hdc) -> Hdc;
    fn SelectObject(hdc: Hdc, obj: isize) -> isize;
    fn GetStockObject(i: i32) -> isize;
    fn SetBkMode(hdc: Hdc, mode: i32) -> i32;
    fn CreateFontW(
        h: i32, w: i32, esc: i32, ori: i32, weight: i32, it: u32, ul: u32, so: u32,
        cs: u32, op: u32, cp: u32, q: u32, paf: u32, face: *const u16,
    ) -> Hfont;
    fn SetTextColor(hdc: Hdc, color: u32) -> u32;
    fn CreateBitmap(w: i32, h: i32, planes: u32, bpp: u32, bits: *const c_void) -> Hbitmap;
    fn DeleteDC(hdc: Hdc) -> i32;
    fn DeleteObject(obj: isize) -> i32;
    fn GdiFlush() -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(msg: u32, data: *mut NotifyIconDataW) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(h: Hkey, sub: *const u16, opt: u32, sam: u32, res: *mut Hkey) -> i32;
    fn RegQueryValueExW(h: Hkey, name: *const u16, res: *mut u32, typ: *mut u32, data: *mut u8, cb: *mut u32) -> i32;
    fn RegSetValueExW(h: Hkey, name: *const u16, res: u32, typ: u32, data: *const u8, cb: u32) -> i32;
    fn RegDeleteValueW(h: Hkey, name: *const u16) -> i32;
    fn RegCloseKey(h: Hkey) -> i32;
}

#[link(name = "setupapi")]
extern "system" {
    fn SetupDiGetClassDevsW(guid: *const Guid, enumerator: *const u16, hwnd: Hwnd, flags: u32) -> Handle;
    fn SetupDiEnumDeviceInfo(set: Handle, idx: u32, data: *mut SpDevinfoData) -> i32;
    fn SetupDiGetDevicePropertyW(
        set: Handle, dev: *const SpDevinfoData, key: *const DevPropKey,
        typ: *mut u32, buf: *mut u8, sz: u32, req: *mut u32, flags: u32,
    ) -> i32;
    fn SetupDiEnumDeviceInterfaces(
        set: Handle, dev: *const SpDevinfoData, guid: *const Guid, idx: u32, data: *mut SpDeviceInterfaceData,
    ) -> i32;
    fn SetupDiGetDeviceInterfaceDetailW(
        set: Handle, ifd: *const SpDeviceInterfaceData, detail: *mut u8, sz: u32,
        req: *mut u32, dev: *mut SpDevinfoData,
    ) -> i32;
    fn SetupDiGetDeviceRegistryPropertyW(
        set: Handle, dev: *const SpDevinfoData, prop: u32, reg_typ: *mut u32,
        buf: *mut u8, sz: u32, req: *mut u32,
    ) -> i32;
    fn SetupDiDestroyDeviceInfoList(set: Handle) -> i32;
}

const PKEY_BT_BATTERY: DevPropKey = DevPropKey {
    fmtid: Guid { data1: 0x104EA319, data2: 0x6EE2, data3: 0x4701, data4: [0xBD, 0x47, 0x8D, 0xDB, 0xF4, 0x25, 0xBB, 0xE5] },
    pid: 2,
};
const PKEY_BT_COD: DevPropKey = DevPropKey {
    fmtid: Guid { data1: 0x2BD67D8B, data2: 0x8BEB, data3: 0x48D5, data4: [0x87, 0xE0, 0x6C, 0xDA, 0x34, 0x28, 0x04, 0x0A] },
    pid: 10,
};
const PKEY_FRIENDLY_NAME: DevPropKey = DevPropKey {
    fmtid: Guid { data1: 0xA45C254E, data2: 0xDF1C, data3: 0x4EFD, data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0] },
    pid: 14,
};
const PKEY_DEVICE_DESC: DevPropKey = DevPropKey {
    fmtid: Guid { data1: 0xA45C254E, data2: 0xDF1C, data3: 0x4EFD, data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0] },
    pid: 2,
};
const PKEY_DEVICE_CLASS: DevPropKey = DevPropKey {
    fmtid: Guid { data1: 0xA45C254E, data2: 0xDF1C, data3: 0x4EFD, data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0] },
    pid: 9,
};
const PKEY_DEVNODE_STATUS: DevPropKey = DevPropKey {
    fmtid: Guid { data1: 0x4340A6C5, data2: 0x93FA, data3: 0x4706, data4: [0x97, 0x2C, 0x7B, 0x64, 0x80, 0x08, 0xA5, 0xA7] },
    pid: 2,
};

#[derive(Clone, Copy, PartialEq)]
enum DeviceKind { Unknown, Mouse, Headphones }

#[derive(Clone, Default)]
struct DeviceState {
    name: String,
    percent: i32,
    online: bool,
    started: bool,
}

#[derive(Default)]
struct AppState {
    mouse: DeviceState,
    headphones: DeviceState,
    mouse_icon: Hicon,
    mouse_added: bool,
    hp_icon: Hicon,
    hp_added: bool,
}

static STATE: OnceLock<Mutex<AppState>> = OnceLock::new();
static HWND_GLOBAL: AtomicIsize = AtomicIsize::new(0);
static REFRESH_EV: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_MSG: AtomicU32 = AtomicU32::new(0);

fn wstr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn clean_name(mut s: String) -> String {
    for sfx in [
        " Hands-Free AG Audio", " Hands-Free Audio", " Hands-Free",
        " Stereo", " Avrcp Transport", " Headphone", " Headset",
    ] {
        if let Some(pos) = s.find(sfx) {
            if pos > 2 {
                s.truncate(pos);
            }
        }
    }
    s.trim().to_string()
}

fn classify(name: &str, class: &str, cod: u32) -> DeviceKind {
    if cod != 0 {
        // Bluetooth Class of Device major-class and pointing-device bits.
        let major = (cod >> 8) & 0x1F;
        if major == 0x04 { return DeviceKind::Headphones; }
        if major == 0x05 && (cod & 0x80) != 0 { return DeviceKind::Mouse; }
    }
    let low = name.to_lowercase();
    let cls = class.to_lowercase();

    for skip in ["keyboard", "клавиатур", "gamepad", "controller", "xbox", "dualsense"] {
        if low.contains(skip) { return DeviceKind::Unknown; }
    }
    for kw in [
        "mouse", "мышь", "мыш", "mx master", "mx anywhere", "g pro", "superlight",
        "g305", "g304", "viper", "deathadder", "basilisk", "orochi", "aerox",
        "lamzu", "pulsar", "attack shark", "vgn", "m720", "m650", "pebble", "trackball",
    ] {
        if low.contains(kw) { return DeviceKind::Mouse; }
    }
    for kw in [
        "headphone", "headset", "earbud", "earphone", "airpods", "buds", "pods",
        "hands-free", "audio", "наушник", "гарнитур", "wh-1000", "wf-1000", "jbl",
        "marshall", "bose", "qc35", "qc45", "sennheiser", "momentum", "beats",
        "barracuda", "blackshark", "arctis", "cloud", "g733", "g435", "freebuds",
        "soundcore", "anker", "edifier", "sony", "major ", "minor ",
    ] {
        if low.contains(kw) { return DeviceKind::Headphones; }
    }
    if cls == "mouse" { return DeviceKind::Mouse; }
    if cls == "audioendpoint" || cls == "media" { return DeviceKind::Headphones; }
    DeviceKind::Unknown
}

fn read_prop_string(set: Handle, dev: &SpDevinfoData, key: &DevPropKey) -> Option<String> {
    unsafe {
        let mut typ = 0u32;
        let mut needed = 0u32;
        SetupDiGetDevicePropertyW(set, dev, key, &mut typ, null_mut(), 0, &mut needed, 0);
        if needed < 2 || needed % 2 != 0 || typ != 0x12 { return None; }
        let mut buf = vec![0u16; (needed as usize + 1) / 2];
        if SetupDiGetDevicePropertyW(set, dev, key, &mut typ, buf.as_mut_ptr() as *mut u8, needed, null_mut(), 0) == 0 {
            return None;
        }
        while buf.last() == Some(&0) { buf.pop(); }
        let s = String::from_utf16_lossy(&buf);
        if s.is_empty() { None } else { Some(s) }
    }
}

fn update_candidate(slot: &mut DeviceState, name: String, percent: i32, started: bool) {
    if !slot.online || (started && !slot.started) {
        slot.name = clean_name(name);
        slot.percent = percent;
        slot.online = true;
        slot.started = started;
    }
}

fn scan_pnp(mouse: &mut DeviceState, hp: &mut DeviceState) {
    unsafe {
        let set = SetupDiGetClassDevsW(null(), null(), 0, DIGCF_ALLCLASSES | DIGCF_PRESENT);
        if set == INVALID_HANDLE_VALUE { return; }

        let mut i = 0u32;
        loop {
            let mut dev = SpDevinfoData {
                cb_size: std::mem::size_of::<SpDevinfoData>() as u32,
                class_guid: Guid { data1: 0, data2: 0, data3: 0, data4: [0; 8] },
                dev_inst: 0,
                reserved: 0,
            };
            if SetupDiEnumDeviceInfo(set, i, &mut dev) == 0 { break; }
            i += 1;

            let mut typ = 0u32;
            let mut bat = 0u8;
            if SetupDiGetDevicePropertyW(set, &dev, &PKEY_BT_BATTERY, &mut typ, &mut bat, 1, null_mut(), 0) == 0 {
                continue;
            }
            if typ != 0x03 || bat > 100 { continue; }

            let mut status = 0u32;
            let mut started = true;
            if SetupDiGetDevicePropertyW(set, &dev, &PKEY_DEVNODE_STATUS, &mut typ, &mut status as *mut u32 as *mut u8, 4, null_mut(), 0) != 0 && typ == 0x07 {
                started = (status & 0x08) != 0;
            }

            let mut cod = 0u32;
            SetupDiGetDevicePropertyW(set, &dev, &PKEY_BT_COD, &mut typ, &mut cod as *mut u32 as *mut u8, 4, null_mut(), 0);

            let name = read_prop_string(set, &dev, &PKEY_FRIENDLY_NAME)
                .or_else(|| read_prop_string(set, &dev, &PKEY_DEVICE_DESC))
                .unwrap_or_default();
            let cls = read_prop_string(set, &dev, &PKEY_DEVICE_CLASS).unwrap_or_default();

            match classify(&name, &cls, cod) {
                DeviceKind::Mouse => update_candidate(mouse, if name.is_empty() { "Bluetooth Mouse".into() } else { name }, bat as i32, started),
                DeviceKind::Headphones => update_candidate(hp, if name.is_empty() { "Bluetooth Headphones".into() } else { name }, bat as i32, started),
                DeviceKind::Unknown => {}
            }
        }
        SetupDiDestroyDeviceInfoList(set);
    }
}

fn scan_ble_gatt_fallback(mouse: &mut DeviceState, hp: &mut DeviceState) {
    if mouse.online && hp.online { return; }
    type FnGetChars = unsafe extern "system" fn(Handle, *const c_void, u16, *mut BthLeGattCharacteristic, *mut u16, u32) -> i32;
    type FnGetVal = unsafe extern "system" fn(Handle, *const BthLeGattCharacteristic, u32, *mut BthLeGattCharacteristicValue, *mut u16, u32) -> i32;

    unsafe {
        let lib = LoadLibraryExW(wstr("BluetoothAPIs.dll").as_ptr(), 0, LOAD_LIBRARY_SEARCH_SYSTEM32);
        if lib == 0 { return; }
        let p_chars = GetProcAddress(lib, b"BluetoothGATTGetCharacteristics\0".as_ptr());
        let p_val = GetProcAddress(lib, b"BluetoothGATTGetCharacteristicValue\0".as_ptr());
        if p_chars.is_null() || p_val.is_null() { FreeLibrary(lib); return; }

        let get_chars: FnGetChars = std::mem::transmute(p_chars);
        let get_val: FnGetVal = std::mem::transmute(p_val);

        let bat_svc = Guid { data1: 0x0000180F, data2: 0x0000, data3: 0x1000, data4: [0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB] };
        let set = SetupDiGetClassDevsW(&bat_svc, null(), 0, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
        if set != INVALID_HANDLE_VALUE {
            let mut idx = 0u32;
            loop {
                let mut ifd = SpDeviceInterfaceData {
                    cb_size: std::mem::size_of::<SpDeviceInterfaceData>() as u32,
                    interface_class_guid: Guid { data1: 0, data2: 0, data3: 0, data4: [0; 8] },
                    flags: 0,
                    reserved: 0,
                };
                if SetupDiEnumDeviceInterfaces(set, null(), &bat_svc, idx, &mut ifd) == 0 { break; }
                idx += 1;

                let mut req = 0u32;
                SetupDiGetDeviceInterfaceDetailW(set, &ifd, null_mut(), 0, &mut req, null_mut());
                let path_offset = 4usize;
                if (req as usize) < path_offset + std::mem::size_of::<u16>() { continue; }
                let mut storage = vec![0u32; (req as usize + 3) / 4];
                let buf = std::slice::from_raw_parts_mut(
                    storage.as_mut_ptr() as *mut u8,
                    storage.len() * std::mem::size_of::<u32>(),
                );
                let detail_cb_size = if std::mem::size_of::<usize>() == 8 { 8u32 } else { 6u32 };
                buf[..4].copy_from_slice(&detail_cb_size.to_ne_bytes());
                let mut di = SpDevinfoData {
                    cb_size: std::mem::size_of::<SpDevinfoData>() as u32,
                    class_guid: Guid { data1: 0, data2: 0, data3: 0, data4: [0; 8] },
                    dev_inst: 0,
                    reserved: 0,
                };
                if SetupDiGetDeviceInterfaceDetailW(set, &ifd, buf.as_mut_ptr(), req, null_mut(), &mut di) == 0 { continue; }
                let path_len = (req as usize - path_offset) / std::mem::size_of::<u16>();
                let path_ptr = buf.as_ptr().add(path_offset) as *const u16;
                let path = std::slice::from_raw_parts(path_ptr, path_len);
                if !path.contains(&0) { continue; }

                let mut name_buf = [0u16; 256];
                let name = if SetupDiGetDeviceRegistryPropertyW(set, &di, 0x0C, null_mut(), name_buf.as_mut_ptr() as *mut u8, 512, null_mut()) != 0 {
                    let len = name_buf.iter().position(|&c| c == 0).unwrap_or(256);
                    String::from_utf16_lossy(&name_buf[..len])
                } else {
                    "BLE Device".to_string()
                };

                let h_svc = CreateFileW(path_ptr, 0x8000_0000, 3, null(), 3, 0x80, 0);
                if h_svc == INVALID_HANDLE_VALUE { continue; }

                let mut cnt = 0u16;
                get_chars(h_svc, null(), 0, null_mut(), &mut cnt, 0);
                if cnt > 0 {
                    let mut chars: Vec<BthLeGattCharacteristic> = vec![std::mem::zeroed(); cnt as usize];
                    if get_chars(h_svc, null(), cnt, chars.as_mut_ptr(), &mut cnt, 0) >= 0 {
                        for ch in &chars {
                            if ch.characteristic_uuid.is_short_uuid != 0
                                && ch.characteristic_uuid.value.short_uuid == 0x2A19
                                && ch.is_readable != 0
                            {
                                let mut vsz = 0u16;
                                get_val(h_svc, ch, 0, null_mut(), &mut vsz, 0);
                                if vsz >= 5 {
                                    let mut value_storage = vec![0u32; (vsz as usize + 3) / 4];
                                    let vp = value_storage.as_mut_ptr() as *mut BthLeGattCharacteristicValue;
                                    if get_val(h_svc, ch, vsz as u32, vp, null_mut(), 0) >= 0
                                        && (*vp).data_size >= 1
                                        && (*vp).data_size as usize <= vsz as usize - std::mem::size_of::<u32>()
                                        && (*vp).data[0] <= 100
                                    {
                                        let pct = (*vp).data[0] as i32;
                                        match classify(&name, "", 0) {
                                            DeviceKind::Headphones if !hp.online => update_candidate(hp, name.clone(), pct, true),
                                            DeviceKind::Mouse | DeviceKind::Unknown if !mouse.online => update_candidate(mouse, name.clone(), pct, true),
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                CloseHandle(h_svc);
            }
            SetupDiDestroyDeviceInfoList(set);
        }
        FreeLibrary(lib);
    }
}

fn is_light_taskbar() -> bool {
    unsafe {
        let mut k: Hkey = 0;
        let sub = wstr(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_QUERY_VALUE, &mut k) != 0 {
            return false;
        }
        let val_name = wstr("SystemUsesLightTheme");
        let mut val = 0u32;
        let mut sz = 4u32;
        RegQueryValueExW(k, val_name.as_ptr(), null_mut(), null_mut(), &mut val as *mut u32 as *mut u8, &mut sz);
        RegCloseKey(k);
        val != 0
    }
}

fn create_native_icon(glyph: u16, percent: i32, online: bool) -> Hicon {
    unsafe {
        let size = GetSystemMetrics(SM_CXSMICON).max(16);
        let light = is_light_taskbar();
        let (fg_r, fg_g, fg_b) = if light { (24u32, 24u32, 24u32) } else { (255u32, 255u32, 255u32) };

        let bi = BitmapInfo {
            bmi_header: BitmapInfoHeader {
                bi_size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                bi_width: size,
                bi_height: -size,
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: 0,
                bi_size_image: 0,
                bi_x_pels_per_meter: 0,
                bi_y_pels_per_meter: 0,
                bi_clr_used: 0,
                bi_clr_important: 0,
            },
            bmi_colors: [0],
        };

        let mut bits: *mut c_void = null_mut();
        let screen = GetDC(0);
        if screen == 0 { return 0; }
        let bmp = CreateDIBSection(screen, &bi, 0, &mut bits, 0, 0);
        if bmp == 0 || bits.is_null() {
            if bmp != 0 { DeleteObject(bmp); }
            ReleaseDC(0, screen);
            return 0;
        }
        let dc = CreateCompatibleDC(screen);
        if dc == 0 {
            DeleteObject(bmp);
            ReleaseDC(0, screen);
            return 0;
        }
        let old_bmp = SelectObject(dc, bmp);
        if old_bmp == 0 || old_bmp == -1 {
            DeleteDC(dc);
            DeleteObject(bmp);
            ReleaseDC(0, screen);
            return 0;
        }

        let full_rc = Rect { left: 0, top: 0, right: size, bottom: size };
        FillRect(dc, &full_rc, GetStockObject(BLACK_BRUSH));
        SetBkMode(dc, TRANSPARENT);

        let bar_h = (size / 8).max(2);
        let glyph_h = size - if online { bar_h + 1 } else { 0 };
        let mut windows_dir = [0u16; 260];
        let windows_len = GetWindowsDirectoryW(windows_dir.as_mut_ptr(), windows_dir.len() as u32) as usize;
        let has_fluent_font = if windows_len > 0 && windows_len < windows_dir.len() {
            PathBuf::from(String::from_utf16_lossy(&windows_dir[..windows_len]))
                .join("Fonts")
                .join("SegoeIcons.ttf")
                .exists()
        } else {
            false
        };
        let face = if has_fluent_font {
            wstr("Segoe Fluent Icons")
        } else {
            wstr("Segoe MDL2 Assets")
        };

        let font = CreateFontW(-glyph_h, 0, 0, 0, 400, 0, 0, 0, 1, 4, 0, 4, 0, face.as_ptr());
        if font == 0 {
            SelectObject(dc, old_bmp);
            DeleteDC(dc);
            DeleteObject(bmp);
            ReleaseDC(0, screen);
            return 0;
        }
        let old_font = SelectObject(dc, font);
        if old_font == 0 || old_font == -1 {
            DeleteObject(font);
            SelectObject(dc, old_bmp);
            DeleteDC(dc);
            DeleteObject(bmp);
            ReleaseDC(0, screen);
            return 0;
        }
        SetTextColor(dc, 0x00FF_FFFF);

        let g_str = [glyph, 0];
        let mut g_rc = Rect { left: 0, top: 0, right: size, bottom: glyph_h };
        DrawTextW(dc, g_str.as_ptr(), 1, &mut g_rc, DRAW_TEXT_FLAGS);

        SelectObject(dc, old_font);
        DeleteObject(font);
        GdiFlush();

        // Icon pixels use premultiplied BGRA.
        let px = std::slice::from_raw_parts_mut(bits as *mut u32, (size * size) as usize);
        let op = if online { 255u32 } else { 110u32 };
        for p in px.iter_mut() {
            let a = ((*p & 0xFF) * op) / 255;
            let pr = (fg_r * a) / 255;
            let pg = (fg_g * a) / 255;
            let pb = (fg_b * a) / 255;
            *p = (a << 24) | (pr << 16) | (pg << 8) | pb;
        }

        if online && percent >= 0 {
            let (r, g, b) = if percent <= 20 { (255u32, 75u32, 75u32) }
                            else if percent <= 40 { (255u32, 185u32, 45u32) }
                            else { (60u32, 215u32, 110u32) };
            let fill_col = 0xFF00_0000 | (r << 16) | (g << 8) | b;
            let bg_col = if light { 0x4400_0000 } else { 0x44FF_FFFF };
            let pad = 1;
            let track_w = (size - pad * 2).max(1);
            let fill_w = ((track_w * percent + 50) / 100).clamp(1, track_w);
            for y in (size - bar_h)..size {
                for x in pad..(size - pad) {
                    px[(y * size + x) as usize] = if (x - pad) < fill_w { fill_col } else { bg_col };
                }
            }
        }

        let mask = vec![0u8; (((size + 15) / 16) * 2 * size) as usize];
        let h_mask = CreateBitmap(size, size, 1, 1, mask.as_ptr() as *const c_void);
        if h_mask == 0 {
            SelectObject(dc, old_bmp);
            DeleteDC(dc);
            DeleteObject(bmp);
            ReleaseDC(0, screen);
            return 0;
        }
        let ii = IconInfo { f_icon: 1, x_hotspot: 0, y_hotspot: 0, hbm_mask: h_mask, hbm_color: bmp };
        let icon = CreateIconIndirect(&ii);

        SelectObject(dc, old_bmp);
        DeleteDC(dc);
        DeleteObject(bmp);
        DeleteObject(h_mask);
        ReleaseDC(0, screen);
        icon
    }
}

fn update_slot(hwnd: Hwnd, uid: u32, glyph: u16, label: &str, dev: &DeviceState, icon_slot: &mut Hicon, added: &mut bool) {
    let icon = create_native_icon(glyph, dev.percent, dev.online);
    if icon == 0 { return; }
    let tip = if dev.online && dev.percent >= 0 {
        if dev.name.is_empty() {
            format!("{}: {}%", label, dev.percent)
        } else {
            format!("{}: {}%\n{}", label, dev.percent, dev.name)
        }
    } else {
        format!("{}: не подключено", label)
    };

    unsafe {
        let mut nid: NotifyIconDataW = std::mem::zeroed();
        nid.cb_size = std::mem::size_of::<NotifyIconDataW>() as u32;
        nid.h_wnd = hwnd;
        nid.u_id = uid;
        nid.u_flags = NIF_ICON | NIF_TIP | NIF_MESSAGE;
        nid.u_callback_message = WM_TRAY;
        nid.h_icon = icon;
        for (dst, src) in nid.sz_tip.iter_mut().take(127).zip(tip.encode_utf16()) {
            *dst = src;
        }
        if Shell_NotifyIconW(if *added { NIM_MODIFY } else { NIM_ADD }, &mut nid) != 0 {
            *added = true;
            if *icon_slot != 0 { DestroyIcon(*icon_slot); }
            *icon_slot = icon;
        } else {
            DestroyIcon(icon);
        }
    }
}

fn refresh_tray() {
    let hwnd = HWND_GLOBAL.load(Ordering::Relaxed);
    if hwnd == 0 { return; }
    if let Ok(mut st) = STATE.get().unwrap().lock() {
        let m = st.mouse.clone();
        let h = st.headphones.clone();
        let (mut mi, mut ma) = (st.mouse_icon, st.mouse_added);
        let (mut hi, mut ha) = (st.hp_icon, st.hp_added);
        update_slot(hwnd, UID_MOUSE, 0xE962, "Мышь", &m, &mut mi, &mut ma);
        update_slot(hwnd, UID_HEADPHONES, 0xE7F6, "Наушники", &h, &mut hi, &mut ha);
        st.mouse_icon = mi; st.mouse_added = ma;
        st.hp_icon = hi; st.hp_added = ha;
    }
}

fn remove_tray() {
    let hwnd = HWND_GLOBAL.load(Ordering::Relaxed);
    if let Ok(mut st) = STATE.get().unwrap().lock() {
        unsafe {
            for (uid, added, icon) in [
                (UID_MOUSE, &mut st.mouse_added, &mut st.mouse_icon),
                (UID_HEADPHONES, &mut st.hp_added, &mut st.hp_icon),
            ] {
                if *added {
                    let mut nid: NotifyIconDataW = std::mem::zeroed();
                    nid.cb_size = std::mem::size_of::<NotifyIconDataW>() as u32;
                    nid.h_wnd = hwnd;
                    nid.u_id = uid;
                    Shell_NotifyIconW(NIM_DELETE, &mut nid);
                    *added = false;
                }
                if *icon != 0 { DestroyIcon(*icon); *icon = 0; }
            }
        }
    }
}

fn is_autostart() -> bool {
    unsafe {
        let mut k: Hkey = 0;
        let sub = wstr(r"Software\Microsoft\Windows\CurrentVersion\Run");
        if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_QUERY_VALUE, &mut k) != 0 { return false; }
        let name = wstr("TrayBatteryRust");
        let mut cb = 0u32;
        let ok = RegQueryValueExW(k, name.as_ptr(), null_mut(), null_mut(), null_mut(), &mut cb) == 0;
        RegCloseKey(k);
        ok
    }
}

fn toggle_autostart() {
    unsafe {
        let mut k: Hkey = 0;
        let sub = wstr(r"Software\Microsoft\Windows\CurrentVersion\Run");
        if RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, KEY_SET_VALUE, &mut k) != 0 { return; }
        let name = wstr("TrayBatteryRust");
        if is_autostart() {
            RegDeleteValueW(k, name.as_ptr());
        } else {
            let mut path = [0u16; 260];
            let len = GetModuleFileNameW(0, path.as_mut_ptr(), 260) as usize;
            let exe = String::from_utf16_lossy(&path[..len]);
            let val = wstr(&format!("\"{}\"", exe));
            RegSetValueExW(k, name.as_ptr(), 0, REG_SZ, val.as_ptr() as *const u8, (val.len() * 2) as u32);
        }
        RegCloseKey(k);
    }
}

unsafe extern "system" fn wnd_proc(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize {
    let tb = TASKBAR_MSG.load(Ordering::Relaxed);
    if tb != 0 && msg == tb {
        if let Ok(mut st) = STATE.get().unwrap().lock() {
            st.mouse_added = false;
            st.hp_added = false;
        }
        refresh_tray();
        return 0;
    }
    match msg {
        WM_UPDATE_UI | WM_SETTINGCHANGE => {
            refresh_tray();
            0
        }
        WM_DEVICECHANGE => {
            if w == DBT_DEVNODES_CHANGED {
                SetTimer(hwnd, TIMER_DEBOUNCE, 2000, null());
            }
            0
        }
        WM_TIMER => {
            if w == TIMER_DEBOUNCE {
                KillTimer(hwnd, TIMER_DEBOUNCE);
                SetEvent(REFRESH_EV.load(Ordering::Relaxed));
            }
            0
        }
        WM_TRAY => {
            let ev = (l as u32) & 0xFFFF;
            if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
                let menu = CreatePopupMenu();
                let s1 = wstr("Обновить сейчас");
                let s2 = wstr("Запускать с Windows");
                let s3 = wstr("Выход");
                AppendMenuW(menu, MF_STRING, 1, s1.as_ptr());
                AppendMenuW(menu, MF_STRING | if is_autostart() { MF_CHECKED } else { MF_UNCHECKED }, 2, s2.as_ptr());
                AppendMenuW(menu, MF_SEPARATOR, 0, null());
                AppendMenuW(menu, MF_STRING, 3, s3.as_ptr());
                let mut pt = Point { x: 0, y: 0 };
                GetCursorPos(&mut pt);
                SetForegroundWindow(hwnd);
                TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, null());
                DestroyMenu(menu);
            }
            0
        }
        WM_COMMAND => {
            match w & 0xFFFF {
                1 => { SetEvent(REFRESH_EV.load(Ordering::Relaxed)); }
                2 => toggle_autostart(),
                3 => { DestroyWindow(hwnd); }
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            remove_tray();
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

fn main() {
    unsafe {
        let mtx = CreateMutexW(null(), 1, wstr("TrayBatteryRust_SingleInstance").as_ptr());
        if mtx == 0 { return; }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mtx);
            return;
        }

        let u32_mod = GetModuleHandleW(wstr("user32.dll").as_ptr());
        let p_dpi = GetProcAddress(u32_mod, b"SetProcessDpiAwarenessContext\0".as_ptr());
        if !p_dpi.is_null() {
            let f: unsafe extern "system" fn(isize) -> i32 = std::mem::transmute(p_dpi);
            f(-4);
        }

        STATE.set(Mutex::new(AppState::default())).ok();
        TASKBAR_MSG.store(RegisterWindowMessageW(wstr("TaskbarCreated").as_ptr()), Ordering::Relaxed);

        let stop_ev = CreateEventW(null(), 1, 0, null());
        let refresh_ev = CreateEventW(null(), 0, 0, null());
        if stop_ev == 0 || refresh_ev == 0 {
            if stop_ev != 0 { CloseHandle(stop_ev); }
            if refresh_ev != 0 { CloseHandle(refresh_ev); }
            ReleaseMutex(mtx);
            CloseHandle(mtx);
            return;
        }
        REFRESH_EV.store(refresh_ev, Ordering::Relaxed);

        let h_inst = GetModuleHandleW(null());
        let cls = wstr("TrayBatteryRustWnd");
        let wc = WndClassW {
            style: 0,
            lpfn_wnd_proc: Some(wnd_proc),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: h_inst,
            h_icon: 0,
            h_cursor: 0,
            hbr_background: 0,
            lpsz_menu_name: null(),
            lpsz_class_name: cls.as_ptr(),
        };
        if RegisterClassW(&wc) == 0 {
            CloseHandle(stop_ev);
            CloseHandle(refresh_ev);
            ReleaseMutex(mtx);
            CloseHandle(mtx);
            return;
        }

        let hwnd = CreateWindowExW(
            0, cls.as_ptr(), wstr("TrayBattery").as_ptr(), 0x00CF_0000,
            0, 0, 0, 0, 0, 0, h_inst, null(),
        );
        if hwnd == 0 {
            CloseHandle(stop_ev);
            CloseHandle(refresh_ev);
            ReleaseMutex(mtx);
            CloseHandle(mtx);
            return;
        }
        HWND_GLOBAL.store(hwnd, Ordering::Relaxed);
        refresh_tray();

        let worker = thread::spawn(move || {
            let handles = [stop_ev, refresh_ev];
            loop {
                let mut m = DeviceState::default();
                let mut hp = DeviceState::default();
                scan_pnp(&mut m, &mut hp);
                scan_ble_gatt_fallback(&mut m, &mut hp);
                if let Ok(mut st) = STATE.get().unwrap().lock() {
                    st.mouse = m;
                    st.headphones = hp;
                }
                PostMessageW(hwnd, WM_UPDATE_UI, 0, 0);
                if WaitForMultipleObjects(2, handles.as_ptr(), 0, POLL_INTERVAL_MS) == 0 {
                    break;
                }
            }
        });

        let mut msg: Msg = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        SetEvent(stop_ev);
        let _ = worker.join();
        remove_tray();
        CloseHandle(stop_ev);
        CloseHandle(refresh_ev);
        ReleaseMutex(mtx);
        CloseHandle(mtx);
    }
}