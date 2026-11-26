use crate::device::{classify_device, sanitize_device_text, store_candidate, DeviceEntry, DeviceKind, DeviceState};
use crate::win32::*;
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};

const VID_LOGITECH: u16 = 0x046D;
const VID_RAZER: u16 = 0x1532;
const HIDPP_SW_ID: u8 = 0x0B;

struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE && !self.0.is_null() {
            unsafe { CloseHandle(self.0); }
        }
    }
}

struct DevInfoGuard(HDEVINFO);
impl Drop for DevInfoGuard {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE && !self.0.is_null() {
            unsafe { SetupDiDestroyDeviceInfoList(self.0); }
        }
    }
}

struct PreparsedGuard(*mut c_void);
impl Drop for PreparsedGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { HidD_FreePreparsedData(self.0); }
        }
    }
}

struct EventGuard(HANDLE);
impl Drop for EventGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0); }
        }
    }
}

#[derive(Clone, Copy)]
struct RazerPidSpec {
    pid: u16,
    tx_id: u8,
    kind: DeviceKind,
    name: &'static str,
}

const RAZER_PIDS: &[RazerPidSpec] = &[
    RazerPidSpec { pid: 0x00A5, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Viper V2 Pro" },
    RazerPidSpec { pid: 0x00A6, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Viper V2 Pro" },
    RazerPidSpec { pid: 0x00C0, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Viper V3 Pro" },
    RazerPidSpec { pid: 0x00C1, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Viper V3 Pro" },
    RazerPidSpec { pid: 0x00B6, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer DeathAdder V3 Pro" },
    RazerPidSpec { pid: 0x00B7, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer DeathAdder V3 Pro" },
    RazerPidSpec { pid: 0x00AA, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Basilisk V3 Pro" },
    RazerPidSpec { pid: 0x00AB, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Basilisk V3 Pro" },
    RazerPidSpec { pid: 0x00AF, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Cobra Pro" },
    RazerPidSpec { pid: 0x00B0, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Cobra Pro" },
    RazerPidSpec { pid: 0x00A7, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Naga V2 Pro" },
    RazerPidSpec { pid: 0x00A8, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Naga V2 Pro" },
    RazerPidSpec { pid: 0x00B8, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer Viper V3 HyperSpeed" },
    RazerPidSpec { pid: 0x00C2, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer DeathAdder V3 HyperSpeed" },
    RazerPidSpec { pid: 0x00C3, tx_id: 0x1F, kind: DeviceKind::Mouse, name: "Razer DeathAdder V3 HyperSpeed" },
    RazerPidSpec { pid: 0x007A, tx_id: 0x3F, kind: DeviceKind::Mouse, name: "Razer Viper Ultimate" },
    RazerPidSpec { pid: 0x007B, tx_id: 0x3F, kind: DeviceKind::Mouse, name: "Razer Viper Ultimate" },
    RazerPidSpec { pid: 0x007C, tx_id: 0x3F, kind: DeviceKind::Mouse, name: "Razer DeathAdder V2 Pro" },
    RazerPidSpec { pid: 0x007D, tx_id: 0x3F, kind: DeviceKind::Mouse, name: "Razer DeathAdder V2 Pro" },
    RazerPidSpec { pid: 0x0086, tx_id: 0x3F, kind: DeviceKind::Mouse, name: "Razer Basilisk Ultimate" },
    RazerPidSpec { pid: 0x0088, tx_id: 0x3F, kind: DeviceKind::Mouse, name: "Razer Basilisk Ultimate" },
    RazerPidSpec { pid: 0x0094, tx_id: 0x3F, kind: DeviceKind::Mouse, name: "Razer Orochi V2" },
    RazerPidSpec { pid: 0x025A, tx_id: 0x3F, kind: DeviceKind::Keyboard, name: "Razer BlackWidow V3 Pro" },
    RazerPidSpec { pid: 0x025C, tx_id: 0x3F, kind: DeviceKind::Keyboard, name: "Razer BlackWidow V3 Pro" },
];

fn lookup_razer_pid(pid: u16) -> Option<RazerPidSpec> {
    RAZER_PIDS.iter().copied().find(|s| s.pid == pid)
}

#[derive(Clone, Copy)]
struct CompxPidSpec {
    vid: u16,
    pid: u16,
    name: &'static str,
}

const COMPX_PIDS: &[CompxPidSpec] = &[
    CompxPidSpec { vid: 0x3554, pid: 0xF503, name: "VGN Dragonfly F1 Pro" },
    CompxPidSpec { vid: 0x3554, pid: 0xFB3E, name: "VGN F2 Pro Max" },
    CompxPidSpec { vid: 0x3554, pid: 0xF58A, name: "VXE R1 Pro Max" },
    CompxPidSpec { vid: 0x3554, pid: 0xF58E, name: "VXE R1 SE+" },
    CompxPidSpec { vid: 0x373B, pid: 0x1085, name: "VXE R1 SE+" },
    CompxPidSpec { vid: 0x373B, pid: 0x104D, name: "VXE MAD R" },
    CompxPidSpec { vid: 0x373B, pid: 0x1040, name: "VXE MAD R" },
    CompxPidSpec { vid: 0x373B, pid: 0x1031, name: "ATK F1 Ultimate" },
    CompxPidSpec { vid: 0x373B, pid: 0x11D9, name: "ATK A9 Ultimate" },
    CompxPidSpec { vid: 0x3554, pid: 0xF524, name: "Zaopin Z2 Mini" },
    CompxPidSpec { vid: 0x3554, pid: 0xF5F7, name: "Scyrox V8" },
    CompxPidSpec { vid: 0x260D, pid: 0x1175, name: "Dareu A950" },
    CompxPidSpec { vid: 0x260D, pid: 0x1154, name: "Dareu A950" },
];

fn lookup_compx_pid(vid: u16, pid: u16) -> Option<CompxPidSpec> {
    COMPX_PIDS.iter().copied().find(|s| s.vid == vid && s.pid == pid)
}

fn is_logitech_receiver_pid(pid: u16) -> bool {
    matches!(pid, 0xC52B | 0xC548 | 0xC547 | 0xC539 | 0xC53F)
}

unsafe fn read_hid_product_string(h: HANDLE) -> String {
    let mut buf = [0u16; 128];
    if HidD_GetProductString(h, buf.as_mut_ptr() as *mut c_void, (buf.len() * 2 - 2) as u32) == 0 {
        return String::new();
    }
    buf[127] = 0;
    sanitize_device_text(&from_wide_slice(&buf), 64)
}

unsafe fn hid_transact_overlapped<F>(
    h: HANDLE,
    tx: &[u8],
    rx: &mut [u8],
    min_match_len: usize,
    mut match_fn: F,
    total_timeout_ms: u32,
) -> bool
where
    F: FnMut(&[u8]) -> bool,
{
    let ev_w = EventGuard(CreateEventW(null_mut(), 1, 0, null()));
    if ev_w.0.is_null() {
        return false;
    }
    let mut ov_w = OVERLAPPED { hEvent: ev_w.0, ..Default::default() };
    let mut written = 0u32;
    let ok_w = WriteFile(
        h,
        tx.as_ptr() as *const c_void,
        tx.len() as u32,
        &mut written,
        &mut ov_w as *mut OVERLAPPED as *mut c_void,
    );
    if ok_w == 0 {
        if GetLastError() != ERROR_IO_PENDING {
            return false;
        }
        if WaitForSingleObject(ov_w.hEvent, total_timeout_ms) != WAIT_OBJECT_0 {
            CancelIoEx(h, &mut ov_w);
            GetOverlappedResult(h, &mut ov_w, &mut written, 1);
            return false;
        }
        if GetOverlappedResult(h, &mut ov_w, &mut written, 0) == 0 {
            return false;
        }
    }

    let deadline = GetTickCount64() + total_timeout_ms as u64;
    let ev_r = EventGuard(CreateEventW(null_mut(), 1, 0, null()));
    if ev_r.0.is_null() {
        return false;
    }
    let mut ov_r = OVERLAPPED { hEvent: ev_r.0, ..Default::default() };

    while GetTickCount64() < deadline {
        ResetEvent(ov_r.hEvent);
        let mut read_bytes = 0u32;
        let ok_r = ReadFile(
            h,
            rx.as_mut_ptr() as *mut c_void,
            rx.len() as u32,
            &mut read_bytes,
            &mut ov_r,
        );
        if ok_r == 0 {
            if GetLastError() != ERROR_IO_PENDING {
                break;
            }
            let now = GetTickCount64();
            let rem = deadline.saturating_sub(now) as u32;
            if WaitForSingleObject(ov_r.hEvent, rem) != WAIT_OBJECT_0 {
                CancelIoEx(h, &mut ov_r);
                GetOverlappedResult(h, &mut ov_r, &mut read_bytes, 1);
                break;
            }
            if GetOverlappedResult(h, &mut ov_r, &mut read_bytes, 0) == 0 {
                break;
            }
        }
        let n = read_bytes as usize;
        if n >= min_match_len && n <= rx.len() && match_fn(&rx[..n]) {
            return true;
        }
    }
    false
}

pub fn voltage_to_percent(mv: i32) -> i32 {
    const LI_ION: &[(i32, i32)] = &[
        (4186, 100), (4067, 90), (3989, 80), (3922, 70), (3859, 60), (3811, 50),
        (3783, 40),  (3763, 30), (3743, 20), (3706, 10), (3663, 5),  (3500, 0),
    ];
    const AA: &[(i32, i32)] = &[
        (1500, 100), (1400, 85), (1300, 65), (1250, 45), (1200, 25), (1100, 10), (1000, 0),
    ];

    let pts = if mv >= 2500 {
        LI_ION
    } else if mv >= 900 {
        AA
    } else {
        return -1;
    };

    if mv >= pts[0].0 {
        return pts[0].1;
    }
    for i in 1..pts.len() {
        if mv >= pts[i].0 {
            let dm = pts[i - 1].0 - pts[i].0;
            let dp = pts[i - 1].1 - pts[i].1;
            if dm <= 0 {
                return pts[i].1;
            }
            return pts[i].1 + ((mv - pts[i].0) * dp + dm / 2) / dm;
        }
    }
    0
}

pub fn parse_hidpp_battery_response(feat_id: u16, resp: &[u8]) -> Option<(i32, bool)> {
    if resp.len() < 7 {
        return None;
    }
    match feat_id {
        0x1004 => {
            let mut pct = resp[4] as i32;
            if pct == 0 {
                let bits = resp[5];
                pct = if (bits & 0x08) != 0 {
                    90
                } else if (bits & 0x04) != 0 {
                    50
                } else if (bits & 0x02) != 0 {
                    20
                } else if (bits & 0x01) != 0 {
                    5
                } else {
                    0
                };
            }
            if !(0..=100).contains(&pct) {
                return None;
            }
            Some((pct, (1..=4).contains(&resp[6])))
        }
        0x1000 => {
            if resp[4] > 100 {
                return None;
            }
            Some((resp[4] as i32, (1..=4).contains(&resp[6])))
        }
        0x1001 => {
            let mv = ((resp[4] as i32) << 8) | (resp[5] as i32);
            let pct = voltage_to_percent(mv);
            if pct < 0 {
                return None;
            }
            Some((pct, (resp[6] & 0x80) != 0))
        }
        _ => None,
    }
}

pub fn razer_calc_crc(buf: &[u8; 91]) -> u8 {
    let mut crc = 0u8;
    for &b in &buf[3..=88] {
        crc ^= b;
    }
    crc
}

pub fn parse_razer_battery_response(resp: &[u8; 91]) -> Option<i32> {
    if resp[1] != 0x02 || resp[7] != 0x07 || resp[8] != 0x80 || resp[89] != razer_calc_crc(resp) {
        return None;
    }
    let pct = ((resp[10] as i32) * 100 + 127) / 255;
    if (0..=100).contains(&pct) { Some(pct) } else { None }
}

pub fn build_compx_battery_request() -> [u8; 17] {
    let mut tx = [0u8; 17];
    tx[0] = 0x08;
    tx[1] = 0x04;
    tx[16] = 0x49;
    tx
}

pub fn parse_compx_battery_response(resp: &[u8]) -> Option<(i32, bool)> {
    if resp.len() < 17 || resp[0] != 0x08 || resp[1] != 0x04 {
        return None;
    }
    let sum: u32 = resp[..17].iter().map(|&b| b as u32).sum();
    if (sum & 0xFF) != 0x55 || resp[6] > 100 {
        return None;
    }
    let pct = resp[6] as i32;
    let charging = resp[7] != 0 && pct < 100;
    Some((pct, charging))
}

unsafe fn hidpp_call(
    h: HANDLE,
    out_cap: usize,
    in_cap: usize,
    dev_idx: u8,
    feat_idx: u8,
    func_id: u8,
    params: &[u8],
) -> Result<Vec<u8>, bool> {
    let mut tx = vec![0u8; out_cap.max(20)];
    tx[0] = 0x11;
    tx[1] = dev_idx;
    tx[2] = feat_idx;
    tx[3] = (func_id << 4) | HIDPP_SW_ID;
    let copy_len = params.len().min(16);
    tx[4..4 + copy_len].copy_from_slice(&params[..copy_len]);

    let mut rx = vec![0u8; in_cap.max(20)];
    let ok = hid_transact_overlapped(
        h,
        &tx,
        &mut rx,
        7,
        |r| {
            if r[0] == 0x10 && r.len() >= 7 && r[1] == dev_idx && (r[2] == 0x8F || r[2] == 0xFF) {
                return true;
            }
            if r[0] == 0x11 && r.len() >= 20 && r[1] == dev_idx {
                if r[2] == 0xFF || r[2] == 0x8F {
                    return true;
                }
                return r[2] == feat_idx && (r[3] & 0x0F) == HIDPP_SW_ID;
            }
            false
        },
        150,
    );
    if !ok {
        return Err(false);
    }
    if rx[2] == 0xFF || rx[2] == 0x8F {
        return Err(true);
    }
    Ok(rx)
}

unsafe fn hidpp_find_feature(
    h: HANDLE,
    out_cap: usize,
    in_cap: usize,
    dev_idx: u8,
    feat_id: u16,
) -> Result<u8, bool> {
    let params = [(feat_id >> 8) as u8, (feat_id & 0xFF) as u8];
    let resp = hidpp_call(h, out_cap, in_cap, dev_idx, 0x00, 0x00, &params)?;
    Ok(resp[4])
}

unsafe fn query_hidpp_device_info(
    h: HANDLE,
    out_cap: usize,
    in_cap: usize,
    dev_idx: u8,
) -> (String, DeviceKind) {
    let mut name = String::new();
    let mut kind = DeviceKind::Unknown;

    let Ok(name_feat) = hidpp_find_feature(h, out_cap, in_cap, dev_idx, 0x0005) else {
        return (name, kind);
    };
    if name_feat == 0 {
        return (name, kind);
    }

    if let Ok(resp) = hidpp_call(h, out_cap, in_cap, dev_idx, name_feat, 0x00, &[]) {
        let name_len = (resp[4] as usize).min(64);
        let mut raw = Vec::with_capacity(name_len);
        while raw.len() < name_len {
            let off = raw.len() as u8;
            let Ok(chunk_resp) = hidpp_call(h, out_cap, in_cap, dev_idx, name_feat, 0x01, &[off]) else {
                break;
            };
            let take_n = (name_len - raw.len()).min(16);
            for &b in &chunk_resp[4..4 + take_n] {
                if b == 0 {
                    break;
                }
                raw.push(b);
            }
            if take_n == 0 {
                break;
            }
        }
        name = sanitize_device_text(&String::from_utf8_lossy(&raw), 64);
    }

    if let Ok(resp) = hidpp_call(h, out_cap, in_cap, dev_idx, name_feat, 0x02, &[]) {
        kind = match resp[4] {
            0 => DeviceKind::Keyboard,
            3 => DeviceKind::Mouse,
            8 => DeviceKind::Headphones,
            _ => DeviceKind::Unknown,
        };
    }

    (name, kind)
}

#[derive(Clone)]
struct HidppCacheEntry {
    bat_feat_id: u16,
    bat_feat_idx: u8,
    name: String,
    kind: DeviceKind,
}

fn hidpp_cache() -> &'static Mutex<HashMap<String, HidppCacheEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<String, HidppCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

unsafe fn probe_logitech_hidpp(
    out: &mut [DeviceState; 3],
    path_u16: &[u16],
    path_key: &str,
    pid: u16,
    out_cap: usize,
    in_cap: usize,
) {
    let h = HandleGuard(CreateFileW(
        path_u16.as_ptr(),
        GENERIC_READ | GENERIC_WRITE,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        null_mut(),
        OPEN_EXISTING,
        FILE_FLAG_OVERLAPPED,
        null_mut(),
    ));
    if h.0 == INVALID_HANDLE_VALUE {
        return;
    }

    let product_fallback = read_hid_product_string(h.0);
    let is_receiver = is_logitech_receiver_pid(pid);
    let mut targets = vec![0xFFu8];
    if is_receiver {
        targets.extend(1u8..=6u8);
    }

    for dev_idx in targets {
        let cache_key = format!("{}#{}", path_key.to_lowercase(), dev_idx);
        let cached = hidpp_cache().lock().ok().and_then(|g| g.get(&cache_key).cloned());

        if let Some(c) = cached {
            let func_id = if c.bat_feat_id == 0x1004 { 0x01 } else { 0x00 };
            if let Ok(resp) = hidpp_call(h.0, out_cap, in_cap, dev_idx, c.bat_feat_idx, func_id, &[]) {
                if let Some((pct, charging)) = parse_hidpp_battery_response(c.bat_feat_id, &resp) {
                    let name = if c.name.is_empty() { product_fallback.clone() } else { c.name };
                    let mut k = c.kind;
                    if k == DeviceKind::Unknown {
                        k = classify_device(&name, "", 0);
                    }
                    if k == DeviceKind::Unknown {
                        k = DeviceKind::Mouse;
                    }
                    store_candidate(
                        out,
                        k,
                        DeviceEntry {
                            key: String::new(),
                            name,
                            container_id: String::new(),
                            bth_addr: 0,
                            percent: pct,
                            charging,
                            active_audio: false,
                            connected: true,
                        },
                    );
                    continue;
                }
            }
            if let Ok(mut g) = hidpp_cache().lock() {
                g.remove(&cache_key);
            }
            continue;
        }

        let mut found = None;
        let mut proto_err = false;
        for &fid in &[0x1004u16, 0x1000u16, 0x1001u16] {
            match hidpp_find_feature(h.0, out_cap, in_cap, dev_idx, fid) {
                Ok(fidx) if fidx != 0 => {
                    found = Some((fid, fidx));
                    break;
                }
                Ok(_) => {}
                Err(true) => {
                    proto_err = true;
                    break;
                }
                Err(false) => {}
            }
        }
        let Some((bat_feat_id, bat_feat_idx)) = found else {
            let _ = proto_err;
            continue;
        };
        if proto_err {
            continue;
        }

        let func_id = if bat_feat_id == 0x1004 { 0x01 } else { 0x00 };
        let Ok(bat_resp) = hidpp_call(h.0, out_cap, in_cap, dev_idx, bat_feat_idx, func_id, &[]) else {
            continue;
        };
        let Some((pct, charging)) = parse_hidpp_battery_response(bat_feat_id, &bat_resp) else {
            continue;
        };

        let (mut dev_name, mut dev_kind) = query_hidpp_device_info(h.0, out_cap, in_cap, dev_idx);
        if dev_name.is_empty() && !is_receiver {
            dev_name = product_fallback.clone();
        }
        if dev_name.is_empty() {
            dev_name = "Logitech Device".to_string();
        }
        if dev_kind == DeviceKind::Unknown {
            dev_kind = classify_device(&dev_name, "", 0);
        }
        if dev_kind == DeviceKind::Unknown {
            dev_kind = DeviceKind::Mouse;
        }

        if let Ok(mut g) = hidpp_cache().lock() {
            if g.len() >= 64 {
                g.clear();
            }
            g.insert(
                cache_key,
                HidppCacheEntry {
                    bat_feat_id,
                    bat_feat_idx,
                    name: dev_name.clone(),
                    kind: dev_kind,
                },
            );
        }

        store_candidate(
            out,
            dev_kind,
            DeviceEntry {
                key: String::new(),
                name: dev_name,
                container_id: String::new(),
                bth_addr: 0,
                percent: pct,
                charging,
                active_audio: false,
                connected: true,
            },
        );

        if !is_receiver && dev_idx == 0xFF {
            break;
        }
    }
}

unsafe fn razer_feature_query(h: HANDLE, tx_id: u8, cmd_id: u8, resp: &mut [u8; 91]) -> bool {
    let mut req = [0u8; 91];
    req[2] = tx_id;
    req[6] = 0x02;
    req[7] = 0x07;
    req[8] = cmd_id;
    req[89] = razer_calc_crc(&req);

    if HidD_SetFeature(h, req.as_mut_ptr() as *mut c_void, req.len() as u32) == 0 {
        return false;
    }
    Sleep(35);

    resp.fill(0);
    if HidD_GetFeature(h, resp.as_mut_ptr() as *mut c_void, resp.len() as u32) == 0 {
        return false;
    }
    resp[1] == 0x02 && resp[7] == 0x07 && resp[8] == cmd_id && resp[89] == razer_calc_crc(resp)
}

unsafe fn probe_razer_device(out: &mut [DeviceState; 3], h: HANDLE, spec: RazerPidSpec) {
    let mut resp = [0u8; 91];
    if !razer_feature_query(h, spec.tx_id, 0x80, &mut resp) {
        return;
    }
    let Some(pct) = parse_razer_battery_response(&resp) else {
        return;
    };
    let mut ch_resp = [0u8; 91];
    let charging = razer_feature_query(h, spec.tx_id, 0x84, &mut ch_resp) && ch_resp[10] != 0;

    let mut name = read_hid_product_string(h);
    if name.is_empty() {
        name = spec.name.to_string();
    }

    store_candidate(
        out,
        spec.kind,
        DeviceEntry {
            key: String::new(),
            name,
            container_id: String::new(),
            bth_addr: 0,
            percent: pct,
            charging,
            active_audio: false,
            connected: true,
        },
    );
}

unsafe fn probe_compx_device(
    out: &mut [DeviceState; 3],
    path_u16: &[u16],
    spec: CompxPidSpec,
    out_cap: usize,
    in_cap: usize,
) {
    let h = HandleGuard(CreateFileW(
        path_u16.as_ptr(),
        GENERIC_READ | GENERIC_WRITE,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        null_mut(),
        OPEN_EXISTING,
        FILE_FLAG_OVERLAPPED,
        null_mut(),
    ));
    if h.0 == INVALID_HANDLE_VALUE {
        return;
    }

    let req = build_compx_battery_request();
    let mut tx = vec![0u8; out_cap.max(17)];
    tx[..17].copy_from_slice(&req);

    let mut rx = vec![0u8; in_cap.max(17)];
    let ok = hid_transact_overlapped(
        h.0,
        &tx,
        &mut rx,
        17,
        |r| parse_compx_battery_response(r).is_some(),
        150,
    );
    if !ok {
        return;
    }
    let Some((pct, charging)) = parse_compx_battery_response(&rx) else {
        return;
    };

    let mut name = read_hid_product_string(h.0);
    if name.is_empty() {
        name = spec.name.to_string();
    }

    store_candidate(
        out,
        DeviceKind::Mouse,
        DeviceEntry {
            key: String::new(),
            name,
            container_id: String::new(),
            bth_addr: 0,
            percent: pct,
            charging,
            active_audio: false,
            connected: true,
        },
    );
}

pub unsafe fn scan_vendor_hid_devices(out: &mut [DeviceState; 3]) {
    let mut hid_guid = GUID::default();
    HidD_GetHidGuid(&mut hid_guid);

    let set = DevInfoGuard(SetupDiGetClassDevsW(
        &hid_guid,
        null(),
        null_mut(),
        DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
    ));
    if set.0 == INVALID_HANDLE_VALUE {
        return;
    }

    let path_offset = size_of::<u32>();
    let detail_cb_size = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
    let mut idx = 0u32;

    loop {
        let mut if_data = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };
        if SetupDiEnumDeviceInterfaces(set.0, null_mut(), &hid_guid, idx, &mut if_data) == 0 {
            break;
        }
        idx += 1;

        let mut req_size = 0u32;
        SetupDiGetDeviceInterfaceDetailW(set.0, &mut if_data, null_mut(), 0, &mut req_size, null_mut());
        if (req_size as usize) < path_offset + 2 {
            continue;
        }

        let mut raw_detail = vec![0u64; ((req_size as usize) + 7) / 8];
        let detail_ptr = raw_detail.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        (*detail_ptr).cbSize = detail_cb_size;

        if SetupDiGetDeviceInterfaceDetailW(set.0, &mut if_data, detail_ptr, req_size, null_mut(), null_mut()) == 0 {
            continue;
        }

        let path_u16_len = ((req_size as usize) - path_offset) / 2;
        if path_u16_len == 0 {
            continue;
        }
        let path_ptr = (detail_ptr as *const u8).add(path_offset) as *const u16;
        let mut path_buf = std::slice::from_raw_parts(path_ptr, path_u16_len).to_vec();
        if path_buf.last().copied() != Some(0) {
            path_buf.push(0);
        }
        let path_key = from_wide_slice(&path_buf);

        let probe_h = HandleGuard(CreateFileW(
            path_buf.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null_mut(),
            OPEN_EXISTING,
            0,
            null_mut(),
        ));
        if probe_h.0 == INVALID_HANDLE_VALUE {
            continue;
        }

        let mut attrs = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            ..Default::default()
        };
        if HidD_GetAttributes(probe_h.0, &mut attrs) == 0 {
            continue;
        }

        let mut ppd_raw: *mut c_void = null_mut();
        if HidD_GetPreparsedData(probe_h.0, &mut ppd_raw) == 0 || ppd_raw.is_null() {
            continue;
        }
        let ppd = PreparsedGuard(ppd_raw);

        let mut caps = HIDP_CAPS::default();
        if HidP_GetCaps(ppd.0, &mut caps) != HIDP_STATUS_SUCCESS {
            continue;
        }

        if attrs.VendorID == VID_LOGITECH
            && caps.UsagePage == 0xFF00
            && caps.InputReportByteLength >= 20
            && caps.OutputReportByteLength >= 20
        {
            probe_logitech_hidpp(
                out,
                &path_buf,
                &path_key,
                attrs.ProductID,
                caps.OutputReportByteLength as usize,
                caps.InputReportByteLength as usize,
            );
            continue;
        }

        if attrs.VendorID == VID_RAZER && caps.FeatureReportByteLength == 91 {
            if let Some(spec) = lookup_razer_pid(attrs.ProductID) {
                probe_razer_device(out, probe_h.0, spec);
            }
            continue;
        }

        if caps.UsagePage == 0xFF02
            && caps.Usage == 0x0002
            && caps.InputReportByteLength >= 17
            && caps.OutputReportByteLength >= 17
        {
            if let Some(spec) = lookup_compx_pid(attrs.VendorID, attrs.ProductID) {
                probe_compx_device(
                    out,
                    &path_buf,
                    spec,
                    caps.OutputReportByteLength as usize,
                    caps.InputReportByteLength as usize,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidpp_unified_battery_and_fallback() {
        let mut resp = [0u8; 20];
        resp[4] = 68;
        resp[6] = 0;
        assert_eq!(parse_hidpp_battery_response(0x1004, &resp), Some((68, false)));

        resp[4] = 0;
        resp[5] = 0x04;
        resp[6] = 2;
        assert_eq!(parse_hidpp_battery_response(0x1004, &resp), Some((50, true)));
    }

    #[test]
    fn hidpp_voltage_curves_liion_and_aa() {
        assert_eq!(voltage_to_percent(4186), 100);
        assert_eq!(voltage_to_percent(3500), 0);
        assert!(voltage_to_percent(3811) == 50);

        assert_eq!(voltage_to_percent(1500), 100);
        assert_eq!(voltage_to_percent(1000), 0);
        assert_eq!(voltage_to_percent(1250), 45);
        assert_eq!(voltage_to_percent(800), -1);

        let mut resp = [0u8; 20];
        resp[4] = (3811 >> 8) as u8;
        resp[5] = (3811 & 0xFF) as u8;
        resp[6] = 0x80;
        assert_eq!(parse_hidpp_battery_response(0x1001, &resp), Some((50, true)));
    }

    #[test]
    fn razer_crc_and_percent() {
        let mut resp = [0u8; 91];
        resp[1] = 0x02;
        resp[2] = 0x1F;
        resp[6] = 0x02;
        resp[7] = 0x07;
        resp[8] = 0x80;
        resp[10] = 255;
        resp[89] = razer_calc_crc(&resp);
        assert_eq!(parse_razer_battery_response(&resp), Some(100));

        resp[10] = 128;
        resp[89] = razer_calc_crc(&resp);
        assert_eq!(parse_razer_battery_response(&resp), Some(50));

        resp[89] ^= 0x01;
        assert_eq!(parse_razer_battery_response(&resp), None);
    }

    #[test]
    fn compx_request_and_response() {
        let req = build_compx_battery_request();
        let req_sum: u32 = req.iter().map(|&b| b as u32).sum();
        assert_eq!(req_sum & 0xFF, 0x55);

        let mut resp = [0u8; 17];
        resp[0] = 0x08;
        resp[1] = 0x04;
        resp[6] = 73;
        resp[7] = 1;
        let partial: u32 = resp[..16].iter().map(|&b| b as u32).sum();
        resp[16] = ((0x55i32 - (partial as i32 & 0xFF)).rem_euclid(256)) as u8;
        assert_eq!(parse_compx_battery_response(&resp), Some((73, true)));

        resp[6] = 100;
        let partial2: u32 = resp[..16].iter().map(|&b| b as u32).sum();
        resp[16] = ((0x55i32 - (partial2 as i32 & 0xFF)).rem_euclid(256)) as u8;
        assert_eq!(parse_compx_battery_response(&resp), Some((100, false)));

        resp[16] ^= 0xFF;
        assert_eq!(parse_compx_battery_response(&resp), None);
    }
}
