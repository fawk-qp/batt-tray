use crate::dbus_ffi::*;
use crate::device::{classify_device, store_candidate, DeviceKind, DeviceState};
use std::ffi::{c_char, c_void};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

fn read_small_file(p: &Path, limit: u64) -> String {
    let Ok(f) = OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        .open(p)
    else {
        return String::new();
    };
    let Ok(meta) = f.metadata() else {
        return String::new();
    };
    if !meta.file_type().is_file() {
        return String::new();
    }
    let mut buf = Vec::with_capacity(limit.min(512) as usize);
    let _ = f.take(limit).read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

fn read_first_line(p: &Path) -> String {
    let raw = read_small_file(p, 255);
    raw.lines().next().unwrap_or("").trim().to_string()
}

unsafe fn scan_upower(sys: *mut DBusConnection, out: &mut [DeviceState; 3]) {
    if sys.is_null() {
        return;
    }
    let Some(call) = DbusMsg::new(dbus_message_new_method_call(
        b"org.freedesktop.UPower\0".as_ptr() as *const c_char,
        b"/org/freedesktop/UPower\0".as_ptr() as *const c_char,
        b"org.freedesktop.UPower\0".as_ptr() as *const c_char,
        b"EnumerateDevices\0".as_ptr() as *const c_char,
    )) else {
        return;
    };

    let mut err = DbusErr::new();
    let Some(rep) = DbusMsg::new(dbus_connection_send_with_reply_and_block(sys, call.0, 1500, &mut err.0)) else {
        return;
    };

    let mut paths = Vec::new();
    let mut it = zero_iter();
    if dbus_message_iter_init(rep.0, &mut it) != 0 && dbus_message_iter_get_arg_type(&mut it) == DBUS_TYPE_ARRAY {
        let mut arr = zero_iter();
        dbus_message_iter_recurse(&mut it, &mut arr);
        while let Some(p) = iter_read_str(&mut arr) {
            paths.push(p);
            dbus_message_iter_next(&mut arr);
        }
    }
    paths.truncate(64);

    for p in paths {
        let cp = cstr(&p);
        let Some(req) = DbusMsg::new(dbus_message_new_method_call(
            b"org.freedesktop.UPower\0".as_ptr() as *const c_char,
            cp.as_ptr(),
            b"org.freedesktop.DBus.Properties\0".as_ptr() as *const c_char,
            b"GetAll\0".as_ptr() as *const c_char,
        )) else {
            continue;
        };
        let mut ait = zero_iter();
        dbus_message_iter_init_append(req.0, &mut ait);
        append_str(&mut ait, "org.freedesktop.UPower.Device");

        err.reset();
        let Some(pr) = DbusMsg::new(dbus_connection_send_with_reply_and_block(sys, req.0, 1000, &mut err.0)) else {
            continue;
        };

        let (mut dev_type, mut state, mut pct) = (0u32, 0u32, -1.0f64);
        let (mut present, mut power_supply) = (false, true);
        let (mut model, mut native, mut icon) = (String::new(), String::new(), String::new());

        let mut pit = zero_iter();
        if dbus_message_iter_init(pr.0, &mut pit) != 0 && dbus_message_iter_get_arg_type(&mut pit) == DBUS_TYPE_ARRAY {
            let mut dict = zero_iter();
            dbus_message_iter_recurse(&mut pit, &mut dict);
            while dbus_message_iter_get_arg_type(&mut dict) == DBUS_TYPE_DICT_ENTRY {
                let mut entry = zero_iter();
                dbus_message_iter_recurse(&mut dict, &mut entry);
                if let Some(key) = iter_read_str(&mut entry) {
                    if dbus_message_iter_next(&mut entry) != 0 && dbus_message_iter_get_arg_type(&mut entry) == DBUS_TYPE_VARIANT {
                        let mut var = zero_iter();
                        dbus_message_iter_recurse(&mut entry, &mut var);
                        let t = dbus_message_iter_get_arg_type(&mut var);
                        match (key.as_str(), t) {
                            ("Type", DBUS_TYPE_UINT32) => dbus_message_iter_get_basic(&mut var, &mut dev_type as *mut u32 as *mut c_void),
                            ("State", DBUS_TYPE_UINT32) => dbus_message_iter_get_basic(&mut var, &mut state as *mut u32 as *mut c_void),
                            ("Percentage", DBUS_TYPE_DOUBLE) => dbus_message_iter_get_basic(&mut var, &mut pct as *mut f64 as *mut c_void),
                            ("IsPresent", DBUS_TYPE_BOOLEAN) => {
                                let mut b = 0u32;
                                dbus_message_iter_get_basic(&mut var, &mut b as *mut u32 as *mut c_void);
                                present = b != 0;
                            }
                            ("PowerSupply", DBUS_TYPE_BOOLEAN) => {
                                let mut b = 1u32;
                                dbus_message_iter_get_basic(&mut var, &mut b as *mut u32 as *mut c_void);
                                power_supply = b != 0;
                            }
                            ("Model", DBUS_TYPE_STRING) => if let Some(s) = iter_read_str(&mut var) { model = s; },
                            ("NativePath", DBUS_TYPE_STRING) => if let Some(s) = iter_read_str(&mut var) { native = s; },
                            ("IconName", DBUS_TYPE_STRING) => if let Some(s) = iter_read_str(&mut var) { icon = s; },
                            _ => {}
                        }
                    }
                }
                dbus_message_iter_next(&mut dict);
            }
        }

        // PowerSupply == true marks laptop main battery or AC adapter; range check also rejects NaN.
        if power_supply || !present || !(0.0..=100.0).contains(&pct) {
            continue;
        }
        let kind = match dev_type {
            5 => DeviceKind::Mouse,
            6 => DeviceKind::Keyboard,
            17 | 18 => DeviceKind::Headphones,
            0 | 2 | 26 => classify_device(&model, &icon, 0),
            _ => DeviceKind::Unknown,
        };
        let rounded = ((pct + 0.5) as i32).clamp(0, 100);
        let name = if model.is_empty() { native } else { model };
        store_candidate(out, kind, name, rounded, state == 1);
    }
}

pub struct AirpodsReading {
    pub display_name: String,
    pub percent: i32,
    pub charging: bool,
}

fn lookup_airpods_model(model: u16) -> Option<&'static str> {
    match model {
        0x0220 => Some("AirPods"),
        0x0F20 => Some("AirPods 2"),
        0x1320 => Some("AirPods 3"),
        0x1920 => Some("AirPods 4"),
        0x0E20 => Some("AirPods Pro"),
        0x1420 | 0x2420 => Some("AirPods Pro 2"),
        0x0A20 => Some("AirPods Max"),
        _ => None,
    }
}

fn nibble_to_percent(nib: u8) -> i32 {
    if nib <= 10 { (nib as i32) * 10 } else { -1 }
}

pub fn parse_apple_continuity_airpods(data: &[u8]) -> Option<AirpodsReading> {
    if data.len() < 2
        || data[0] != 0x07
        || data[1] < 0x19
        || data.len() < (data[1] as usize) + 2
        || data[2] != 0x01
    {
        return None;
    }
    let model = ((data[3] as u16) << 8) | (data[4] as u16);
    let model_name = lookup_airpods_model(model)?;

    let flipped = (data[5] & 0x20) != 0;
    let nib_a = (data[6] >> 4) & 0x0F;
    let nib_b = data[6] & 0x0F;
    let (left_nib, right_nib) = if flipped { (nib_b, nib_a) } else { (nib_a, nib_b) };
    let case_nib = data[7] & 0x0F;
    let chg_flags = (data[7] >> 4) & 0x0F;
    let chg_a = (chg_flags & 0x01) != 0;
    let chg_b = (chg_flags & 0x02) != 0;
    let (left_chg, right_chg) = if flipped { (chg_b, chg_a) } else { (chg_a, chg_b) };

    let left_pct = nibble_to_percent(left_nib);
    let right_pct = nibble_to_percent(right_nib);
    let case_pct = nibble_to_percent(case_nib);

    let (min_pct, charging) = match (left_pct >= 0, right_pct >= 0) {
        (true, true) => (left_pct.min(right_pct), left_chg || right_chg),
        (true, false) => (left_pct, left_chg),
        (false, true) => (right_pct, right_chg),
        (false, false) => return None,
    };

    let mut parts: Vec<String> = Vec::new();
    if model == 0x0A20 {
        parts.push(format!("{}%", min_pct));
    } else {
        if left_pct >= 0 {
            parts.push(format!("L:{}%", left_pct));
        }
        if right_pct >= 0 {
            parts.push(format!("R:{}%", right_pct));
        }
        if case_pct >= 0 {
            parts.push(format!("C:{}%", case_pct));
        }
    }

    let display_name = if parts.is_empty() {
        model_name.to_string()
    } else {
        format!("{} ({})", model_name, parts.join(" "))
    };
    Some(AirpodsReading {
        display_name,
        percent: min_pct,
        charging,
    })
}

unsafe fn read_apple_manufacturer_data(arr_iter: *mut DBusMessageIter) -> Vec<u8> {
    let mut dict = zero_iter();
    dbus_message_iter_recurse(arr_iter, &mut dict);
    while dbus_message_iter_get_arg_type(&mut dict) == DBUS_TYPE_DICT_ENTRY {
        let mut entry = zero_iter();
        dbus_message_iter_recurse(&mut dict, &mut entry);
        if dbus_message_iter_get_arg_type(&mut entry) == DBUS_TYPE_UINT16 {
            let mut mfg_id = 0u16;
            dbus_message_iter_get_basic(&mut entry, &mut mfg_id as *mut u16 as *mut c_void);
            if mfg_id == 0x004C
                && dbus_message_iter_next(&mut entry) != 0
                && dbus_message_iter_get_arg_type(&mut entry) == DBUS_TYPE_VARIANT
            {
                let mut var = zero_iter();
                dbus_message_iter_recurse(&mut entry, &mut var);
                if dbus_message_iter_get_arg_type(&mut var) == DBUS_TYPE_ARRAY {
                    let mut bytes = zero_iter();
                    dbus_message_iter_recurse(&mut var, &mut bytes);
                    let mut out = Vec::new();
                    while dbus_message_iter_get_arg_type(&mut bytes) == DBUS_TYPE_BYTE && out.len() < 64 {
                        let mut b = 0u8;
                        dbus_message_iter_get_basic(&mut bytes, &mut b as *mut u8 as *mut c_void);
                        out.push(b);
                        dbus_message_iter_next(&mut bytes);
                    }
                    return out;
                }
            }
        }
        dbus_message_iter_next(&mut dict);
    }
    Vec::new()
}

unsafe fn scan_bluez(sys: *mut DBusConnection, out: &mut [DeviceState; 3], allow_airpods_ble: bool) {
    if sys.is_null() || (out[0].online && out[1].online && out[2].online) {
        return;
    }
    let Some(call) = DbusMsg::new(dbus_message_new_method_call(
        b"org.bluez\0".as_ptr() as *const c_char,
        b"/\0".as_ptr() as *const c_char,
        b"org.freedesktop.DBus.ObjectManager\0".as_ptr() as *const c_char,
        b"GetManagedObjects\0".as_ptr() as *const c_char,
    )) else {
        return;
    };

    let mut err = DbusErr::new();
    let Some(rep) = DbusMsg::new(dbus_connection_send_with_reply_and_block(sys, call.0, 1500, &mut err.0)) else {
        return;
    };

    let mut it = zero_iter();
    if dbus_message_iter_init(rep.0, &mut it) == 0 || dbus_message_iter_get_arg_type(&mut it) != DBUS_TYPE_ARRAY {
        return;
    }

    struct AirpodsCandidate {
        reading: AirpodsReading,
        rssi: i16,
    }
    let mut paired_candidates: Vec<AirpodsCandidate> = Vec::new();
    let mut unpaired_candidates: Vec<AirpodsCandidate> = Vec::new();

    let mut objs = zero_iter();
    dbus_message_iter_recurse(&mut it, &mut objs);
    while dbus_message_iter_get_arg_type(&mut objs) == DBUS_TYPE_DICT_ENTRY {
        let mut oe = zero_iter();
        dbus_message_iter_recurse(&mut objs, &mut oe);
        if dbus_message_iter_next(&mut oe) != 0 && dbus_message_iter_get_arg_type(&mut oe) == DBUS_TYPE_ARRAY {
            let mut ifaces = zero_iter();
            dbus_message_iter_recurse(&mut oe, &mut ifaces);
            let (mut connected, mut paired, mut has_bat, mut has_rssi) = (false, false, false, false);
            let (mut pct, mut cod, mut rssi) = (0u8, 0u32, -127i16);
            let (mut name, mut icon) = (String::new(), String::new());
            let mut apple_data: Vec<u8> = Vec::new();

            while dbus_message_iter_get_arg_type(&mut ifaces) == DBUS_TYPE_DICT_ENTRY {
                let mut ie = zero_iter();
                dbus_message_iter_recurse(&mut ifaces, &mut ie);
                if let Some(if_name) = iter_read_str(&mut ie) {
                    let is_dev = if_name == "org.bluez.Device1";
                    let is_bat = if_name == "org.bluez.Battery1";
                    if (is_dev || is_bat)
                        && dbus_message_iter_next(&mut ie) != 0
                        && dbus_message_iter_get_arg_type(&mut ie) == DBUS_TYPE_ARRAY
                    {
                        let mut props = zero_iter();
                        dbus_message_iter_recurse(&mut ie, &mut props);
                        while dbus_message_iter_get_arg_type(&mut props) == DBUS_TYPE_DICT_ENTRY {
                            let mut pe = zero_iter();
                            dbus_message_iter_recurse(&mut props, &mut pe);
                            if let Some(pk) = iter_read_str(&mut pe) {
                                if dbus_message_iter_next(&mut pe) != 0
                                    && dbus_message_iter_get_arg_type(&mut pe) == DBUS_TYPE_VARIANT
                                {
                                    let mut pv = zero_iter();
                                    dbus_message_iter_recurse(&mut pe, &mut pv);
                                    let pt = dbus_message_iter_get_arg_type(&mut pv);
                                    match (is_dev, is_bat, pk.as_str(), pt) {
                                        (_, true, "Percentage", DBUS_TYPE_BYTE) => {
                                            dbus_message_iter_get_basic(&mut pv, &mut pct as *mut u8 as *mut c_void);
                                            has_bat = true;
                                        }
                                        (true, _, "Connected", DBUS_TYPE_BOOLEAN) => {
                                            let mut b = 0u32;
                                            dbus_message_iter_get_basic(&mut pv, &mut b as *mut u32 as *mut c_void);
                                            connected = b != 0;
                                        }
                                        (true, _, "Paired", DBUS_TYPE_BOOLEAN) => {
                                            let mut b = 0u32;
                                            dbus_message_iter_get_basic(&mut pv, &mut b as *mut u32 as *mut c_void);
                                            paired = b != 0;
                                        }
                                        (true, _, "Name", DBUS_TYPE_STRING) => if let Some(s) = iter_read_str(&mut pv) { name = s; },
                                        (true, _, "Icon", DBUS_TYPE_STRING) => if let Some(s) = iter_read_str(&mut pv) { icon = s; },
                                        (true, _, "Class", DBUS_TYPE_UINT32) => dbus_message_iter_get_basic(&mut pv, &mut cod as *mut u32 as *mut c_void),
                                        (true, _, "RSSI", DBUS_TYPE_INT16) => {
                                            dbus_message_iter_get_basic(&mut pv, &mut rssi as *mut i16 as *mut c_void);
                                            has_rssi = true;
                                        }
                                        (true, _, "ManufacturerData", DBUS_TYPE_ARRAY) => {
                                            apple_data = read_apple_manufacturer_data(&mut pv);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            dbus_message_iter_next(&mut props);
                        }
                    }
                }
                dbus_message_iter_next(&mut ifaces);
            }
            if connected && has_bat && pct <= 100 {
                let kind = classify_device(&name, &icon, cod);
                store_candidate(out, kind, name, pct as i32, false);
            }
            if !apple_data.is_empty() {
                if paired && connected {
                    if let Some(reading) = parse_apple_continuity_airpods(&apple_data) {
                        paired_candidates.push(AirpodsCandidate {
                            reading,
                            rssi: if has_rssi { rssi } else { 0 },
                        });
                    }
                } else if !paired && allow_airpods_ble && has_rssi && rssi >= -65 {
                    if let Some(reading) = parse_apple_continuity_airpods(&apple_data) {
                        unpaired_candidates.push(AirpodsCandidate { reading, rssi });
                    }
                }
            }
        }
        dbus_message_iter_next(&mut objs);
    }

    let chosen = if !paired_candidates.is_empty() {
        paired_candidates.into_iter().max_by_key(|c| c.rssi)
    } else if allow_airpods_ble && !unpaired_candidates.is_empty() {
        unpaired_candidates.into_iter().max_by_key(|c| c.rssi)
    } else {
        None
    };
    if let Some(best) = chosen {
        store_candidate(
            out,
            DeviceKind::Headphones,
            best.reading.display_name,
            best.reading.percent,
            best.reading.charging,
        );
    }
}

fn scan_sysfs(out: &mut [DeviceState; 3]) {
    if out[0].online && out[1].online && out[2].online {
        return;
    }
    let Ok(entries) = fs::read_dir("/sys/class/power_supply") else {
        return;
    };
    for entry in entries.flatten() {
        let base = entry.path();
        if read_first_line(&base.join("scope")) != "Device" {
            continue;
        }
        let Ok(pct) = read_first_line(&base.join("capacity")).parse::<i32>() else {
            continue;
        };
        if !(0..=100).contains(&pct) {
            continue;
        }

        let charging = read_first_line(&base.join("status")) == "Charging";
        let mut name = read_first_line(&base.join("model_name"));
        if name.is_empty() {
            let uev = read_small_file(&base.join("device/uevent"), 512);
            for line in uev.lines() {
                if let Some(v) = line.strip_prefix("HID_NAME=") {
                    name = v.trim().to_string();
                    break;
                }
            }
        }

        // Kernel input capability heuristic: EV_REL indicates mouse, LEDs indicate keyboard.
        let (mut has_rel, mut has_led) = (false, false);
        if let Ok(inps) = fs::read_dir(base.join("device/input")) {
            for ie in inps.flatten() {
                if !ie.file_name().to_string_lossy().starts_with("input") {
                    continue;
                }
                let ip = ie.path();
                if name.is_empty() {
                    name = read_first_line(&ip.join("name"));
                }
                if read_first_line(&ip.join("capabilities/rel")).chars().any(|c| c.is_ascii_hexdigit() && c != '0') {
                    has_rel = true;
                }
                if read_first_line(&ip.join("capabilities/led")).chars().any(|c| c.is_ascii_hexdigit() && c != '0') {
                    has_led = true;
                }
            }
        }

        let mut kind = classify_device(&name, "", 0);
        if kind == DeviceKind::Unknown {
            if has_rel && !has_led {
                kind = DeviceKind::Mouse;
            } else if has_led && !has_rel {
                kind = DeviceKind::Keyboard;
            }
        }
        if name.is_empty() {
            name = entry.file_name().to_string_lossy().into_owned();
        }
        store_candidate(out, kind, name, pct, charging);
    }
}

pub fn scan_all_devices(sys_bus: *mut DBusConnection, allow_airpods_ble: bool) -> [DeviceState; 3] {
    let mut out: [DeviceState; 3] = Default::default();
    unsafe {
        scan_upower(sys_bus, &mut out);
        scan_bluez(sys_bus, &mut out, allow_airpods_ble);
    }
    scan_sysfs(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_continuity_airpods_pro_and_flip() {
        // Model 0x1420 (AirPods Pro 2), not flipped: nib_a=8 (L=80%), nib_b=9 (R=90%), case=6 (C=60%), charging L=1
        let mut pkt = [0u8; 27];
        pkt[..9].copy_from_slice(&[0x07, 0x19, 0x01, 0x14, 0x20, 0x00, 0x89, 0x16, 0x00]);
        let r = parse_apple_continuity_airpods(&pkt).unwrap();
        assert_eq!(r.percent, 80);
        assert!(r.charging);
        assert_eq!(r.display_name, "AirPods Pro 2 (L:80% R:90% C:60%)");

        // Flipped (data[5] & 0x20): nib_a=3 (now R=30%), nib_b=7 (now L=70%), chg_b=1 (now L charging)
        let mut pkt_flip = [0u8; 27];
        pkt_flip[..9].copy_from_slice(&[0x07, 0x19, 0x01, 0x14, 0x20, 0x20, 0x37, 0x2F, 0x00]);
        let rf = parse_apple_continuity_airpods(&pkt_flip).unwrap();
        assert_eq!(rf.percent, 30);
        assert!(rf.charging);
        assert_eq!(rf.display_name, "AirPods Pro 2 (L:70% R:30%)");
    }

    #[test]
    fn apple_continuity_airpods_max_and_invalid() {
        let mut pkt_max = [0u8; 27];
        pkt_max[..9].copy_from_slice(&[0x07, 0x19, 0x01, 0x0A, 0x20, 0x00, 0x6F, 0x0F, 0x00]);
        let rm = parse_apple_continuity_airpods(&pkt_max).unwrap();
        assert_eq!(rm.percent, 60);
        assert!(!rm.charging);
        assert_eq!(rm.display_name, "AirPods Max (60%)");

        // Truncated 9-byte packet with data[1] == 0x19 must be rejected
        assert!(parse_apple_continuity_airpods(&[0x07, 0x19, 0x01, 0x14, 0x20, 0x00, 0x89, 0x16, 0x00]).is_none());

        // Unknown model or wrong header rejected
        let mut bad_model = [0u8; 27];
        bad_model[..9].copy_from_slice(&[0x07, 0x19, 0x01, 0x99, 0x99, 0x00, 0x55, 0x05, 0x00]);
        assert!(parse_apple_continuity_airpods(&bad_model).is_none());

        let mut bad_mode = [0u8; 27];
        bad_mode[..9].copy_from_slice(&[0x07, 0x19, 0x02, 0x14, 0x20, 0x00, 0x55, 0x05, 0x00]);
        assert!(parse_apple_continuity_airpods(&bad_mode).is_none());
        assert!(parse_apple_continuity_airpods(&[0x07, 0x19, 0x01, 0x14, 0x20]).is_none());
    }
}