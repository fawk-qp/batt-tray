#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Mouse = 0,
    Keyboard = 1,
    Headphones = 2,
    Unknown = 3,
}

#[derive(Clone, Default)]
pub struct DeviceState {
    pub name: String,
    pub percent: i32,
    pub online: bool,
    pub charging: bool,
}

pub struct SlotSpec {
    pub kind: DeviceKind,
    pub uid: u32,
    pub label: &'static str,
    pub glyph: u16,
}

pub const SLOT_SPECS: [SlotSpec; 3] = [
    SlotSpec { kind: DeviceKind::Mouse,      uid: 1001, label: "Мышь",       glyph: 0xE962 },
    SlotSpec { kind: DeviceKind::Keyboard,   uid: 1002, label: "Клавиатура", glyph: 0xE765 },
    SlotSpec { kind: DeviceKind::Headphones, uid: 1003, label: "Наушники",   glyph: 0xE7F6 },
];

pub fn classify_device(name: &str, pnp_class: &str, cod: u32) -> DeviceKind {
    let low = name.to_lowercase();
    for skip in ["gamepad", "controller", "xbox", "dualsense", "dualshock", "joystick", "joy-con"] {
        if low.contains(skip) {
            return DeviceKind::Unknown;
        }
    }

    // Bluetooth Class of Device: Major 0x04 = Audio, 0x05 = Peripheral (0x80 mouse, 0x40 keyboard)
    if cod != 0 {
        let major = (cod >> 8) & 0x1F;
        if major == 0x04 {
            return DeviceKind::Headphones;
        }
        if major == 0x05 {
            if (cod & 0x80) != 0 {
                return DeviceKind::Mouse;
            }
            if (cod & 0x40) != 0 {
                return DeviceKind::Keyboard;
            }
        }
    }

    for kw in [
        "keyboard", "kbd", "keychron", "nuphy", "akko", "wooting", "anne pro", "rk61", "rk84",
        "mx keys", "k380", "k780", "g915", "g715", "apex pro", "blackwidow", "huntsman",
        "magic keyboard", "клавиатур",
    ] {
        if low.contains(kw) {
            return DeviceKind::Keyboard;
        }
    }

    for kw in [
        "mouse", "mx master", "mx anywhere", "g pro", "superlight", "g305", "g304", "g502",
        "viper", "deathadder", "basilisk", "orochi", "aerox", "lamzu", "pulsar",
        "attack shark", "vgn", "m720", "m650", "pebble", "trackball", "мышь",
    ] {
        if low.contains(kw) {
            return DeviceKind::Mouse;
        }
    }

    for kw in [
        "headphone", "headset", "earbud", "earphone", "airpods", "buds", "pods", "hands-free",
        "wh-1000", "wf-1000", "jbl", "marshall", "bose", "qc35", "qc45", "sennheiser",
        "momentum", "beats", "barracuda", "blackshark", "arctis", "cloud", "g733", "g435",
        "freebuds", "soundcore", "anker", "edifier", "sony", "наушник", "гарнитур",
    ] {
        if low.contains(kw) {
            return DeviceKind::Headphones;
        }
    }

    let cls = pnp_class.to_lowercase();
    if cls == "mouse" {
        return DeviceKind::Mouse;
    }
    if cls == "keyboard" {
        return DeviceKind::Keyboard;
    }
    if cls == "audioendpoint" || cls == "media" {
        return DeviceKind::Headphones;
    }

    DeviceKind::Unknown
}

pub fn store_candidate(out: &mut [DeviceState; 3], kind: DeviceKind, name: &str, percent: i32, charging: bool) {
    if kind == DeviceKind::Unknown || !(0..=100).contains(&percent) {
        return;
    }
    let dst = &mut out[kind as usize];
    if !dst.online {
        dst.name = name.trim().to_string();
        dst.percent = percent;
        dst.online = true;
        dst.charging = charging;
    }
}