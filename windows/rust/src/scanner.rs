use crate::device::{classify_device, store_candidate, DeviceState};
use crate::win32::*;
use std::mem::{size_of, transmute};
use std::ptr::{null, null_mut};

const PROP_BLUETOOTH_BATTERY: DEVPROPKEY = DEVPROPKEY {
    fmtid: GUID { data1: 0x104EA319, data2: 0x6EE2, data3: 0x4701, data4: [0xBD, 0x47, 0x8D, 0xDB, 0xF4, 0x25, 0xBB, 0xE5] },
    pid: 2,
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

// Standard Bluetooth LE GATT Battery Service UUID (0x180F)
const GUID_BTHLE_BATTERY_SERVICE: GUID = GUID {
    data1: 0x0000180F,
    data2: 0x0000,
    data3: 0x1000,
    data4: [0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB],
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

unsafe fn scan_pnp_bluetooth(out: &mut [DeviceState; 3]) {
    let set = DevInfoGuard(SetupDiGetClassDevsW(null(), null(), null_mut(), DIGCF_ALLCLASSES | DIGCF_PRESENT));
    if set.0 == INVALID_HANDLE_VALUE {
        return;
    }

    let mut idx = 0u32;
    loop {
        let mut dev = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
        if SetupDiEnumDeviceInfo(set.0, idx, &mut dev) == 0 {
            break;
        }
        idx += 1;

        let Some(pct) = read_dev_prop_byte(set.0, &mut dev, &PROP_BLUETOOTH_BATTERY) else {
            continue;
        };
        if pct > 100 {
            continue;
        }

        let name = resolve_dev_name(set.0, &mut dev);
        let cls = read_dev_prop_string(set.0, &mut dev, &PROP_DEVICE_CLASS);
        let cod = read_dev_prop_u32(set.0, &mut dev, &PROP_BLUETOOTH_COD);
        store_candidate(out, classify_device(&name, &cls, cod), &name, pct as i32, false);
    }
}

unsafe fn scan_ble_gatt(out: &mut [DeviceState; 3]) {
    if out[0].online && out[1].online && out[2].online {
        return;
    }

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

        // Aligned u64 backing store guarantees proper alignment for SP_DEVICE_INTERFACE_DETAIL_DATA_W
        let mut raw_detail = vec![0u64; ((req_size as usize) + 7) / 8];
        let detail_ptr = raw_detail.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
        (*detail_ptr).cbSize = detail_cb_size;

        let mut dev = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
        if SetupDiGetDeviceInterfaceDetailW(set.0, &mut if_data, detail_ptr, req_size, null_mut(), &mut dev) == 0 {
            continue;
        }

        let path_bytes = (req_size as usize) - path_offset;
        let path_u16_len = path_bytes / 2;
        if path_u16_len == 0 {
            continue;
        }
        let path_ptr = (detail_ptr as *const u8).add(path_offset) as *const u16;
        let path_slice = std::slice::from_raw_parts(path_ptr, path_u16_len);
        let mut path_buf = path_slice.to_vec();
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
                        let cod = read_dev_prop_u32(set.0, &mut dev, &PROP_BLUETOOTH_COD);
                        store_candidate(out, classify_device(&name, &cls, cod), &name, pct as i32, false);
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
        scan_pnp_bluetooth(&mut out);
        scan_ble_gatt(&mut out);
    }
    out
}