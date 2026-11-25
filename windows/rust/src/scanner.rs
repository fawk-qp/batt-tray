use crate::device::{classify_device, finalize_devices, names_match_audio, store_candidate, DeviceEntry, DeviceKind, DeviceState};
use crate::win32::*;
use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::{size_of, transmute};
use std::ptr::{null, null_mut};

const PROP_BLUETOOTH_BATTERY: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID { data1: 0x104EA319, data2: 0x6EE2, data3: 0x4701, data4: [0xBD, 0x47, 0x8D, 0xDB, 0xF4, 0x25, 0xBB, 0xE5] },
    pid: 2,
};
const PROP_BLUETOOTH_ADDRESS: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID { data1: 0x2BD67D8B, data2: 0x8BEB, data3: 0x48D5, data4: [0x87, 0xE0, 0x6C, 0xDA, 0x34, 0x28, 0x04, 0x0A] },
    pid: 1,
};
const PROP_BLUETOOTH_COD: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID { data1: 0x2BD67D8B, data2: 0x8BEB, data3: 0x48D5, data4: [0x87, 0xE0, 0x6C, 0xDA, 0x34, 0x28, 0x04, 0x0A] },
    pid: 10,
};
const PROP_FRIENDLY_NAME: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID { data1: 0xA45C254E, data2: 0xDF1C, data3: 0x4EFD, data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0] },
    pid: 14,
};
const PROP_DEVICE_DESC: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID { data1: 0xA45C254E, data2: 0xDF1C, data3: 0x4EFD, data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0] },
    pid: 2,
};
const PROP_DEVICE_CLASS: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID { data1: 0xA45C254E, data2: 0xDF1C, data3: 0x4EFD, data4: [0x80, 0x20, 0x67, 0xD1, 0x46, 0xA8, 0x50, 0xE0] },
    pid: 9,
};
const PROP_CONTAINER_ID: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID { data1: 0x8C7ED206, data2: 0x3F8A, data3: 0x4827, data4: [0xB3, 0xAB, 0xAE, 0x9E, 0x1F, 0xAE, 0xFC, 0x6C] },
    pid: 2,
};
const PROP_IS_CONNECTED: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID { data1: 0x83DA6326, data2: 0x97A6, data3: 0x4088, data4: [0x94, 0x53, 0xA1, 0x92, 0x3F, 0x57, 0x3B, 0x29] },
    pid: 15,
};

const CLSID_MM_DEVICE_ENUMERATOR: GUID = GUID {
    data1: 0xBCDE0395, data2: 0xE52F, data3: 0x467C, data4: [0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E],
};
const IID_IMM_DEVICE_ENUMERATOR: GUID = GUID {
    data1: 0xA95664D2, data2: 0x9614, data3: 0x4F35, data4: [0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6],
};
const GUID_BTHLE_BATTERY_SERVICE: GUID = GUID {
    data1: 0x0000180F, data2: 0x0000, data3: 0x1000, data4: [0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB],
};

struct DevInfoGuard(HDEVINFO);
impl Drop for DevInfoGuard {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE && !self.0.is_null() {
            unsafe { SetupDiDestroyDeviceInfoList(self.0); }
        }
    }
}

struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE && !self.0.is_null() {
            unsafe { CloseHandle(self.0); }
        }
    }
}

struct ModuleGuard(HMODULE);
impl Drop for ModuleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { FreeLibrary(self.0); }
        }
    }
}

#[derive(Default)]
struct ActiveAudioInfo {
    container_id: String,
    friendly_name: String,
}

fn format_guid_lower(g: &GUID) -> String {
    if g.data1 == 0 && g.data2 == 0 && g.data3 == 0 && g.data4 == [0; 8] {
        return String::new();
    }
    format!(
        "{{{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}}}",
        g.data1, g.data2, g.data3,
        g.data4[0], g.data4[1], g.data4[2], g.data4[3],
        g.data4[4], g.data4[5], g.data4[6], g.data4[7],
    )
}

fn parse_bth_address(raw: &str) -> u64 {
    let hex: String = raw.chars().filter(|&c| c != ':' && c != '-' && c != ' ').collect();
    if hex.len() != 12 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return 0;
    }
    u64::from_str_radix(&hex, 16).unwrap_or(0)
}

unsafe fn extract_bth_addr_from_instance_id(set: HDEVINFO, dev: &mut SP_DEVINFO_DATA) -> u64 {
    let mut buf = [0u16; 256];
    if SetupDiGetDeviceInstanceIdW(set, dev, buf.as_mut_ptr(), buf.len() as u32, null_mut()) == 0 {
        return 0;
    }
    let inst = from_wide_slice(&buf).to_lowercase();
    for tag in ["dev_", "bluetoothdevice_"] {
        if let Some(pos) = inst.find(tag) {
            let start = pos + tag.len();
            if start + 12 <= inst.len() && inst.is_char_boundary(start) && inst.is_char_boundary(start + 12) {
                let addr = parse_bth_address(&inst[start..start + 12]);
                if addr != 0 {
                    return addr;
                }
            }
        }
    }
    0
}

unsafe fn resolve_bth_address(set: HDEVINFO, dev: &mut SP_DEVINFO_DATA) -> u64 {
    let addr = parse_bth_address(&read_dev_prop_string(set, dev, &PROP_BLUETOOTH_ADDRESS));
    if addr != 0 {
        addr
    } else {
        extract_bth_addr_from_instance_id(set, dev)
    }
}

unsafe fn read_pwstr(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while len < 512 && *ptr.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
}

unsafe fn query_active_audio_endpoint() -> ActiveAudioInfo {
    let mut info = ActiveAudioInfo::default();
    let mut enumerator: *mut IMMDeviceEnumerator = null_mut();
    let hr = CoCreateInstance(
        &CLSID_MM_DEVICE_ENUMERATOR,
        null_mut(),
        CLSCTX_INPROC_SERVER,
        &IID_IMM_DEVICE_ENUMERATOR,
        &mut enumerator as *mut *mut IMMDeviceEnumerator as *mut *mut c_void,
    );
    if hr < 0 || enumerator.is_null() {
        return info;
    }

    let mut dev: *mut IMMDevice = null_mut();
    let mut ghr = ((*(*enumerator).lpVtbl).GetDefaultAudioEndpoint)(enumerator, 0, 0, &mut dev);
    if ghr < 0 || dev.is_null() {
        ghr = ((*(*enumerator).lpVtbl).GetDefaultAudioEndpoint)(enumerator, 0, 1, &mut dev);
    }

    if ghr >= 0 && !dev.is_null() {
        let mut store: *mut IPropertyStore = null_mut();
        if ((*(*dev).lpVtbl).OpenPropertyStore)(dev, STGM_READ, &mut store) >= 0 && !store.is_null() {
            let mut pv = PROPVARIANT::default();
            if ((*(*store).lpVtbl).GetValue)(store, &PROP_CONTAINER_ID, &mut pv) >= 0 {
                if pv.vt == VT_LPWSTR && !pv.ptr.is_null() {
                    let cid = read_pwstr(pv.ptr as *const u16).to_lowercase();
                    if cid != "{00000000-0000-0000-0000-000000000000}" {
                        info.container_id = cid;
                    }
                } else if pv.vt == VT_CLSID && !pv.ptr.is_null() {
                    info.container_id = format_guid_lower(&*(pv.ptr as *const GUID));
                }
            }
            PropVariantClear(&mut pv);

            let mut pv2 = PROPVARIANT::default();
            if ((*(*store).lpVtbl).GetValue)(store, &PROP_FRIENDLY_NAME, &mut pv2) >= 0
                && pv2.vt == VT_LPWSTR
                && !pv2.ptr.is_null()
            {
                info.friendly_name = read_pwstr(pv2.ptr as *const u16);
            }
            PropVariantClear(&mut pv2);

            ((*(*store).lpVtbl).Release)(store);
        }
        ((*(*dev).lpVtbl).Release)(dev);
    }

    ((*(*enumerator).lpVtbl).Release)(enumerator);
    info
}

fn matches_active_audio(audio: &ActiveAudioInfo, cid: &str, name: &str) -> bool {
    if !audio.container_id.is_empty() && !cid.is_empty() && audio.container_id == cid {
        return true;
    }
    names_match_audio(name, &audio.friendly_name)
}

unsafe fn read_dev_prop_string(set: HDEVINFO, dev: &mut SP_DEVINFO_DATA, key: &DEVPROPKEY) -> String {
    let mut prop_type = 0u32;
    let mut req_bytes = 0u32;
    SetupDiGetDevicePropertyW(set, dev, key, &mut prop_type, null_mut(), 0, &mut req_bytes, 0);
    if req_bytes < 2 || prop_type != DEVPROP_TYPE_STRING {
        return String::new();
    }

    let words = (req_bytes as usize / 2) + 1;
    let mut buf = vec![0u16; words];
    let ok = SetupDiGetDevicePropertyW(
        set,
        dev,
        key,
        &mut prop_type,
        buf.as_mut_ptr() as *mut u8,
        req_bytes,
        &mut req_bytes,
        0,
    );
    if ok == 0 {
        return String::new();
    }
    if let Some(last) = buf.last_mut() {
        *last = 0;
    }
    from_wide_slice(&buf)
}

unsafe fn read_dev_prop_byte(set: HDEVINFO, dev: &mut SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<u8> {
    let mut prop_type = 0u32;
    let mut val = 0u8;
    let mut req = 0u32;
    let ok = SetupDiGetDevicePropertyW(set, dev, key, &mut prop_type, &mut val, 1, &mut req, 0);
    if ok != 0 && prop_type == DEVPROP_TYPE_BYTE && req >= 1 {
        Some(val)
    } else {
        None
    }
}

unsafe fn read_dev_prop_bool(set: HDEVINFO, dev: &mut SP_DEVINFO_DATA, key: &DEVPROPKEY) -> i8 {
    let mut prop_type = 0u32;
    let mut val = 0u8;
    let mut req = 0u32;
    let ok = SetupDiGetDevicePropertyW(set, dev, key, &mut prop_type, &mut val, 1, &mut req, 0);
    if ok != 0 && prop_type == DEVPROP_TYPE_BOOLEAN && req >= 1 {
        if val != 0 { 1 } else { -1 }
    } else {
        0
    }
}

unsafe fn read_dev_prop_guid(set: HDEVINFO, dev: &mut SP_DEVINFO_DATA, key: &DEVPROPKEY) -> String {
    let mut prop_type = 0u32;
    let mut g = GUID::default();
    let mut req = 0u32;
    let ok = SetupDiGetDevicePropertyW(
        set,
        dev,
        key,
        &mut prop_type,
        &mut g as *mut GUID as *mut u8,
        size_of::<GUID>() as u32,
        &mut req,
        0,
    );
    if ok != 0 && prop_type == DEVPROP_TYPE_GUID && req >= size_of::<GUID>() as u32 {
        format_guid_lower(&g)
    } else {
        String::new()
    }
}

unsafe fn read_dev_prop_u32(set: HDEVINFO, dev: &mut SP_DEVINFO_DATA, key: &DEVPROPKEY) -> u32 {
    let mut prop_type = 0u32;
    let mut val = 0u32;
    let mut req = 0u32;
    let ok = SetupDiGetDevicePropertyW(
        set,
        dev,
        key,
        &mut prop_type,
        &mut val as *mut u32 as *mut u8,
        size_of::<u32>() as u32,
        &mut req,
        0,
    );
    if ok != 0 && prop_type == DEVPROP_TYPE_UINT32 && req >= size_of::<u32>() as u32 {
        val
    } else {
        0
    }
}

unsafe fn resolve_dev_name(set: HDEVINFO, dev: &mut SP_DEVINFO_DATA) -> String {
    let name = read_dev_prop_string(set, dev, &PROP_FRIENDLY_NAME);
    if !name.is_empty() {
        name
    } else {
        read_dev_prop_string(set, dev, &PROP_DEVICE_DESC)
    }
}

unsafe fn scan_pnp_bluetooth(out: &mut [DeviceState; 3], audio: &ActiveAudioInfo) {
    let set = DevInfoGuard(SetupDiGetClassDevsW(null(), null(), null_mut(), DIGCF_ALLCLASSES | DIGCF_PRESENT));
    if set.0 == INVALID_HANDLE_VALUE {
        return;
    }

    struct RawCand {
        name: String,
        pnp_class: String,
        container_id: String,
        bth_addr: u64,
        cod: u32,
        pct: i32,
        node_conn: i8,
    }

    let mut container_conn: HashMap<String, i8> = HashMap::new();
    let mut container_addr: HashMap<String, u64> = HashMap::new();
    let mut raw: Vec<RawCand> = Vec::new();
    let mut idx = 0u32;

    loop {
        let mut dev = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
        if SetupDiEnumDeviceInfo(set.0, idx, &mut dev) == 0 {
            break;
        }
        idx += 1;

        let cid = read_dev_prop_guid(set.0, &mut dev, &PROP_CONTAINER_ID);
        let conn_state = read_dev_prop_bool(set.0, &mut dev, &PROP_IS_CONNECTED);
        let node_addr = resolve_bth_address(set.0, &mut dev);
        if !cid.is_empty() {
            if conn_state != 0 {
                let entry = container_conn.entry(cid.clone()).or_insert(0);
                if conn_state > 0 {
                    *entry = 1;
                } else if *entry == 0 {
                    *entry = -1;
                }
            }
            if node_addr != 0 {
                container_addr.entry(cid.clone()).or_insert(node_addr);
            }
        }

        let Some(pct) = read_dev_prop_byte(set.0, &mut dev, &PROP_BLUETOOTH_BATTERY) else {
            continue;
        };
        if pct > 100 {
            continue;
        }

        raw.push(RawCand {
            name: resolve_dev_name(set.0, &mut dev),
            pnp_class: read_dev_prop_string(set.0, &mut dev, &PROP_DEVICE_CLASS),
            container_id: cid,
            bth_addr: node_addr,
            cod: read_dev_prop_u32(set.0, &mut dev, &PROP_BLUETOOTH_COD),
            pct: pct as i32,
            node_conn: conn_state,
        });
    }

    for cand in raw {
        let state = if !cand.container_id.is_empty() {
            container_conn.get(&cand.container_id).copied().unwrap_or(cand.node_conn)
        } else {
            cand.node_conn
        };
        // Skip paired Bluetooth devices explicitly reported as disconnected (stale cached battery)
        if state < 0 {
            continue;
        }

        let addr = if cand.bth_addr != 0 {
            cand.bth_addr
        } else if !cand.container_id.is_empty() {
            container_addr.get(&cand.container_id).copied().unwrap_or(0)
        } else {
            0
        };

        let is_active = matches_active_audio(audio, &cand.container_id, &cand.name);
        let mut kind = classify_device(&cand.name, &cand.pnp_class, cand.cod);
        if kind == DeviceKind::Unknown && is_active {
            kind = DeviceKind::Headphones;
        }

        store_candidate(
            out,
            kind,
            DeviceEntry {
                key: String::new(),
                name: cand.name,
                container_id: cand.container_id,
                bth_addr: addr,
                percent: cand.pct,
                charging: false,
                active_audio: is_active,
                connected: state > 0,
            },
        );
    }
}

unsafe fn scan_ble_gatt(out: &mut [DeviceState; 3], audio: &ActiveAudioInfo) {
    let dll_name = to_wide("BluetoothAPIs.dll");
    let dll = ModuleGuard(LoadLibraryExW(dll_name.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32));
    if dll.0.is_null() {
        return;
    }

    let p_chars = GetProcAddress(dll.0, b"BluetoothGATTGetCharacteristics\0".as_ptr() as *const i8);
    let p_val = GetProcAddress(dll.0, b"BluetoothGATTGetCharacteristicValue\0".as_ptr() as *const i8);
    if p_chars.is_null() || p_val.is_null() {
        return;
    }
    let get_chars: FnBluetoothGattGetCharacteristics = transmute(p_chars);
    let get_char_val: FnBluetoothGattGetCharacteristicValue = transmute(p_val);

    let set = DevInfoGuard(SetupDiGetClassDevsW(
        &GUID_BTHLE_BATTERY_SERVICE,
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
        if SetupDiEnumDeviceInterfaces(set.0, null_mut(), &GUID_BTHLE_BATTERY_SERVICE, idx, &mut if_data) == 0 {
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

        let mut dev = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
        if SetupDiGetDeviceInterfaceDetailW(set.0, &mut if_data, detail_ptr, req_size, null_mut(), &mut dev) == 0 {
            continue;
        }
        if read_dev_prop_bool(set.0, &mut dev, &PROP_IS_CONNECTED) < 0 {
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

        let h = HandleGuard(CreateFileW(
            path_buf.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null_mut(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            null_mut(),
        ));
        if h.0 == INVALID_HANDLE_VALUE {
            continue;
        }

        let mut char_count = 0u16;
        let _ = get_chars(h.0, null_mut(), 0, null_mut(), &mut char_count, BLUETOOTH_GATT_FLAG_NONE);
        if char_count == 0 {
            continue;
        }

        let mut chars = vec![BTH_LE_GATT_CHARACTERISTIC::default(); char_count as usize];
        let mut actual = 0u16;
        if get_chars(h.0, null_mut(), char_count, chars.as_mut_ptr(), &mut actual, BLUETOOTH_GATT_FLAG_NONE) < 0 {
            continue;
        }

        let limit = (actual as usize).min(chars.len());
        for ch in chars.iter_mut().take(limit) {
            let short_uuid = (ch.CharacteristicUuid.Value.data1 & 0xFFFF) as u16;
            if ch.CharacteristicUuid.IsShortUuid == 0 || short_uuid != 0x2A19 {
                continue;
            }

            let mut val_size = 0u16;
            let _ = get_char_val(h.0, ch, 0, null_mut(), &mut val_size, BLUETOOTH_GATT_FLAG_NONE);
            let data_offset = size_of::<u32>();
            if (val_size as usize) < data_offset + 1 {
                continue;
            }

            let mut raw_val = vec![0u64; ((val_size as usize) + 7) / 8];
            let val_ptr = raw_val.as_mut_ptr() as *mut BTH_LE_GATT_CHARACTERISTIC_VALUE;
            if get_char_val(h.0, ch, val_size as u32, val_ptr, null_mut(), BLUETOOTH_GATT_FLAG_NONE) >= 0 {
                let data_size = (*val_ptr).DataSize as usize;
                let max_payload = (val_size as usize) - data_offset;
                if data_size >= 1 && data_size <= max_payload {
                    let pct = *(val_ptr as *const u8).add(data_offset);
                    if pct <= 100 {
                        let name = resolve_dev_name(set.0, &mut dev);
                        let cls = read_dev_prop_string(set.0, &mut dev, &PROP_DEVICE_CLASS);
                        let cid = read_dev_prop_guid(set.0, &mut dev, &PROP_CONTAINER_ID);
                        let addr = resolve_bth_address(set.0, &mut dev);
                        let cod = read_dev_prop_u32(set.0, &mut dev, &PROP_BLUETOOTH_COD);
                        let is_active = matches_active_audio(audio, &cid, &name);
                        let mut kind = classify_device(&name, &cls, cod);
                        if kind == DeviceKind::Unknown && is_active {
                            kind = DeviceKind::Headphones;
                        }

                        store_candidate(
                            out,
                            kind,
                            DeviceEntry {
                                key: String::new(),
                                name,
                                container_id: cid,
                                bth_addr: addr,
                                percent: pct as i32,
                                charging: false,
                                active_audio: is_active,
                                connected: true,
                            },
                        );
                        break;
                    }
                }
            }
        }
    }
}

pub fn scan_all_devices() -> [DeviceState; 3] {
    let mut out: [DeviceState; 3] = Default::default();
    unsafe {
        let audio = query_active_audio_endpoint();
        scan_pnp_bluetooth(&mut out, &audio);
        scan_ble_gatt(&mut out, &audio);
    }
    finalize_devices(&mut out);
    out
}

pub fn disconnect_bluetooth_device(bth_addr: u64) -> bool {
    if bth_addr == 0 {
        return false;
    }
    unsafe {
        let dll_name = to_wide("BluetoothAPIs.dll");
        let dll = ModuleGuard(LoadLibraryExW(dll_name.as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32));
        if dll.0.is_null() {
            return false;
        }

        let p_first = GetProcAddress(dll.0, b"BluetoothFindFirstRadio\0".as_ptr() as *const i8);
        let p_next = GetProcAddress(dll.0, b"BluetoothFindNextRadio\0".as_ptr() as *const i8);
        let p_close = GetProcAddress(dll.0, b"BluetoothFindRadioClose\0".as_ptr() as *const i8);
        if p_first.is_null() || p_next.is_null() || p_close.is_null() {
            return false;
        }
        let find_first: FnBluetoothFindFirstRadio = transmute(p_first);
        let find_next: FnBluetoothFindNextRadio = transmute(p_next);
        let find_close: FnBluetoothFindRadioClose = transmute(p_close);

        let params = BLUETOOTH_FIND_RADIO_PARAMS { dwSize: size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32 };
        let mut h_radio: HANDLE = null_mut();
        let h_find = find_first(&params, &mut h_radio);
        if h_find.is_null() {
            return false;
        }

        // IOCTL_BTH_DISCONNECT_DEVICE (FILE_DEVICE_BLUETOOTH=0x41, function=0x03, METHOD_BUFFERED)
        const IOCTL_BTH_DISCONNECT_DEVICE: u32 = 0x0041_000C;
        let mut disconnected = false;
        loop {
            if !h_radio.is_null() {
                let mut bytes = 0u32;
                if DeviceIoControl(
                    h_radio,
                    IOCTL_BTH_DISCONNECT_DEVICE,
                    &bth_addr as *const u64 as *const c_void,
                    size_of::<u64>() as u32,
                    null_mut(),
                    0,
                    &mut bytes,
                    null_mut(),
                ) != 0
                {
                    disconnected = true;
                }
                CloseHandle(h_radio);
                h_radio = null_mut();
            }
            if find_next(h_find, &mut h_radio) == 0 {
                break;
            }
        }
        find_close(h_find);
        disconnected
    }
}