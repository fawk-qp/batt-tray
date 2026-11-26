#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Mouse = 0,
    Keyboard = 1,
    Headphones = 2,
    Unknown = 3,
}

#[derive(Clone, Default)]
pub struct DeviceEntry {
    pub key: String,
    pub name: String,
    pub percent: i32,
    pub charging: bool,
}

#[derive(Clone, Default)]
pub struct DeviceState {
    pub name: String,
    pub percent: i32,
    pub online: bool,
    pub charging: bool,
    pub items: Vec<DeviceEntry>,
}

pub struct SlotSpec {
    pub kind: DeviceKind,
    pub id: &'static str,
    pub label: &'static str,
    pub icon_name: &'static str,
}

pub const SLOT_SPECS: [SlotSpec; 3] = [
    SlotSpec { kind: DeviceKind::Mouse,      id: "batt-tray-mouse",      label: "Мышь",       icon_name: "input-mouse-symbolic" },
    SlotSpec { kind: DeviceKind::Keyboard,   id: "batt-tray-keyboard",   label: "Клавиатура", icon_name: "input-keyboard-symbolic" },
    SlotSpec { kind: DeviceKind::Headphones, id: "batt-tray-headphones", label: "Наушники",   icon_name: "audio-headphones-symbolic" },
];

pub fn sanitize_device_text(raw: &str, max_chars: usize) -> String {
    raw.chars()
        .filter(|&c| {
            !c.is_control()
                && !(c == '\u{061C}'
                    || ('\u{200B}'..='\u{200F}').contains(&c)
                    || ('\u{202A}'..='\u{202E}').contains(&c)
                    || ('\u{2066}'..='\u{2069}').contains(&c)
                    || c == '\u{FEFF}')
        })
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn classify_device(name: &str, icon_hint: &str, cod: u32) -> DeviceKind {
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

    let ic = icon_hint.to_lowercase();
    if ic.contains("mouse") {
        return DeviceKind::Mouse;
    }
    if ic.contains("keyboard") {
        return DeviceKind::Keyboard;
    }
    if ic.contains("head") || ic.contains("audio") {
        return DeviceKind::Headphones;
    }

    let low = name.to_lowercase();
    for skip in ["gamepad", "controller", "xbox", "dualsense", "dualshock", "joystick", "joy-con"] {
        if low.contains(skip) {
            return DeviceKind::Unknown;
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

    DeviceKind::Unknown
}

pub fn store_candidate(out: &mut [DeviceState; 3], kind: DeviceKind, name: String, percent: i32, charging: bool) {
    if kind == DeviceKind::Unknown || !(0..=100).contains(&percent) {
        return;
    }
    let clean = sanitize_device_text(&name, 64);
    let key = clean.to_lowercase();

    let dst = &mut out[kind as usize];
    if !key.is_empty() {
        if let Some(existing) = dst.items.iter_mut().find(|it| it.key == key) {
            existing.charging |= charging;
        } else if dst.items.len() < 16 {
            dst.items.push(DeviceEntry {
                key,
                name: clean.clone(),
                percent,
                charging,
            });
        }
    }

    if !dst.online {
        dst.name = clean;
        dst.percent = percent;
        dst.online = true;
        dst.charging = charging;
    }
}