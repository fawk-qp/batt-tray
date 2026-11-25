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

unsafe fn scan_bluez(sys: *mut DBusConnection, out: &mut [DeviceState; 3]) {
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

    let mut objs = zero_iter();
    dbus_message_iter_recurse(&mut it, &mut objs);
    while dbus_message_iter_get_arg_type(&mut objs) == DBUS_TYPE_DICT_ENTRY {
        let mut oe = zero_iter();
        dbus_message_iter_recurse(&mut objs, &mut oe);
        if dbus_message_iter_next(&mut oe) != 0 && dbus_message_iter_get_arg_type(&mut oe) == DBUS_TYPE_ARRAY {
            let mut ifaces = zero_iter();
            dbus_message_iter_recurse(&mut oe, &mut ifaces);
            let (mut connected, mut has_bat, mut pct, mut cod) = (false, false, 0u8, 0u32);
            let (mut name, mut icon) = (String::new(), String::new());

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
                                        (true, _, "Name", DBUS_TYPE_STRING) => if let Some(s) = iter_read_str(&mut pv) { name = s; },
                                        (true, _, "Icon", DBUS_TYPE_STRING) => if let Some(s) = iter_read_str(&mut pv) { icon = s; },
                                        (true, _, "Class", DBUS_TYPE_UINT32) => dbus_message_iter_get_basic(&mut pv, &mut cod as *mut u32 as *mut c_void),
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
        }
        dbus_message_iter_next(&mut objs);
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

pub fn scan_all_devices(sys_bus: *mut DBusConnection) -> [DeviceState; 3] {
    let mut out: [DeviceState; 3] = Default::default();
    unsafe {
        scan_upower(sys_bus, &mut out);
        scan_bluez(sys_bus, &mut out);
    }
    scan_sysfs(&mut out);
    out
}