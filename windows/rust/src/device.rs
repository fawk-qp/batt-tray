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
    pub container_id: String,
    pub bth_addr: u64,
    pub percent: i32,
    pub charging: bool,
    pub active_audio: bool,
    pub connected: bool,
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
    pub uid: u32,
    pub label: &'static str,
    pub glyph: u16,
}

pub const SLOT_SPECS: [SlotSpec; 3] = [
    SlotSpec { kind: DeviceKind::Mouse,      uid: 1001, label: "Мышь",       glyph: 0xE962 },
    SlotSpec { kind: DeviceKind::Keyboard,   uid: 1002, label: "Клавиатура", glyph: 0xE765 },
    SlotSpec { kind: DeviceKind::Headphones, uid: 1003, label: "Наушники",   glyph: 0xE7F6 },
];

pub fn sanitize_device_text(raw: &str, max_chars: usize) -> String {
    raw.chars()
        .filter(|&c| {
            !c.is_control()
                && !(('\u{200B}'..='\u{200F}').contains(&c)
                    || ('\u{202A}'..='\u{202E}').contains(&c)
                    || ('\u{2066}'..='\u{2069}').contains(&c)
                    || c == '\u{FEFF}')
        })
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn clean_device_name(raw: &str) -> String {
    let mut s = sanitize_device_text(raw, 80);
    const SUFFIXES: [&str; 6] = [
        "hands-free ag audio",
        "hands-free ag",
        "hands-free audio",
        "hands-free",
        "avrcp transport",
        "stereo",
    ];
    let mut changed = true;
    while changed {
        changed = false;
        let low = s.to_lowercase();
        for suf in SUFFIXES {
            if low.ends_with(suf) && s.len() >= suf.len() {
                let cut = s.len() - suf.len();
                if s.is_char_boundary(cut) {
                    s.truncate(cut);
                    s = s.trim().trim_end_matches(['-', ':', '–']).trim().to_string();
                    changed = true;
                    break;
                }
            }
        }
    }
    sanitize_device_text(&s, 64)
}

pub fn names_match_audio(dev_name: &str, audio_endpoint_name: &str) -> bool {
    let dev = clean_device_name(dev_name).to_lowercase();
    let audio = clean_device_name(audio_endpoint_name).to_lowercase();
    if dev.chars().count() < 4 || audio.chars().count() < 4 {
        return false;
    }
    if audio.contains(&dev) || dev.contains(&audio) {
        return true;
    }
    if let (Some(open), Some(close)) = (audio.find('('), audio.rfind(')')) {
        if close > open + 1 {
            let inner = clean_device_name(&audio[open + 1..close]).to_lowercase();
            if inner.chars().count() >= 4 && (dev.contains(&inner) || inner.contains(&dev)) {
                return true;
            }
        }
    }
    false
}

pub fn classify_device(name: &str, pnp_class: &str, cod: u32) -> DeviceKind {
    let low = name.to_lowercase();
    for skip in ["gamepad", "controller", "xbox", "dualsense", "dualshock", "joystick", "joy-con"] {
        if low.contains(skip) {
            return DeviceKind::Unknown;
        }
    }

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

pub fn store_candidate(out: &mut [DeviceState; 3], kind: DeviceKind, mut entry: DeviceEntry) {
    if kind == DeviceKind::Unknown || !(0..=100).contains(&entry.percent) {
        return;
    }
    entry.name = clean_device_name(&entry.name);
    entry.container_id = sanitize_device_text(&entry.container_id.to_lowercase(), 48);
    let norm = entry.name.to_lowercase();
    entry.key = if !entry.container_id.is_empty() {
        entry.container_id.clone()
    } else {
        norm.clone()
    };
    if entry.key.is_empty() {
        return;
    }

    let items = &mut out[kind as usize].items;
    for existing in items.iter_mut() {
        let same_cid = !entry.container_id.is_empty() && entry.container_id == existing.container_id;
        let same_name = !norm.is_empty() && norm == existing.name.to_lowercase();
        if same_cid || same_name {
            existing.active_audio |= entry.active_audio;
            existing.connected |= entry.connected;
            existing.charging |= entry.charging;
            if entry.bth_addr != 0 && existing.bth_addr == 0 {
                existing.bth_addr = entry.bth_addr;
            }
            if existing.name.is_empty() && !entry.name.is_empty() {
                existing.name = entry.name;
            }
            if existing.key.is_empty() {
                existing.key = entry.key;
            }
            return;
        }
    }
    if items.len() >= 16 {
        return;
    }
    items.push(entry);
}

pub fn finalize_devices(out: &mut [DeviceState; 3]) {
    for slot in out.iter_mut() {
        if slot.items.is_empty() {
            slot.online = false;
            slot.percent = -1;
            slot.charging = false;
            slot.name.clear();
            continue;
        }
        slot.items.sort_by(|a, b| {
            b.active_audio.cmp(&a.active_audio).then(b.connected.cmp(&a.connected))
        });
        slot.online = true;
        slot.name = slot.items[0].name.clone();
        slot.percent = slot.items[0].percent;
        slot.charging = slot.items[0].charging;
    }
}