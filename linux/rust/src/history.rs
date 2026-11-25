use crate::dbus_ffi::{O_CLOEXEC, O_NOFOLLOW, O_NONBLOCK};
use crate::device::{sanitize_device_text, DeviceEntry, DeviceKind, DeviceState};
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_FILE_BYTES: u64 = 16_384;
const MAX_HISTORY_ITEMS: usize = 32;
const TIMESTAMP_REFRESH_SEC: i64 = 300;

#[derive(Clone)]
pub struct HistoryItem {
    pub kind: DeviceKind,
    pub key: String,
    pub name: String,
    pub percent: i32,
    pub last_seen_unix: i64,
}

#[derive(Default)]
pub struct DeviceHistory {
    items: Vec<HistoryItem>,
    demo_mode: bool,
}

fn now_unix_sec() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn matches_active(item: &HistoryItem, active_items: &[DeviceEntry]) -> bool {
    let item_low = item.name.to_lowercase();
    for act in active_items {
        if !item.key.is_empty() && item.key == act.key {
            return true;
        }
        if !item_low.is_empty() && item_low == act.name.to_lowercase() {
            return true;
        }
    }
    false
}

impl DeviceHistory {
    fn storage_dir() -> Option<PathBuf> {
        if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
            if xdg.starts_with('/') {
                return Some(PathBuf::from(xdg).join("batt-tray"));
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            if home.starts_with('/') {
                return Some(PathBuf::from(home).join(".local").join("state").join("batt-tray"));
            }
        }
        None
    }

    fn storage_path() -> Option<PathBuf> {
        Self::storage_dir().map(|d| d.join("history.tsv"))
    }

    pub fn load(&mut self, demo_mode: bool) {
        self.items.clear();
        self.demo_mode = demo_mode;
        if self.demo_mode {
            let now = now_unix_sec();
            self.items.push(HistoryItem {
                kind: DeviceKind::Headphones,
                key: "sony wh-1000xm5".to_string(),
                name: "Sony WH-1000XM5".to_string(),
                percent: 64,
                last_seen_unix: now - 7200,
            });
            self.items.push(HistoryItem {
                kind: DeviceKind::Mouse,
                key: "mx master 3s".to_string(),
                name: "MX Master 3S".to_string(),
                percent: 91,
                last_seen_unix: now - 90_000,
            });
            return;
        }

        let Some(path) = Self::storage_path() else {
            return;
        };
        let Ok(file) = OpenOptions::new()
            .read(true)
            .custom_flags(O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK)
            .open(&path)
        else {
            return;
        };
        let Ok(meta) = file.metadata() else {
            return;
        };
        if !meta.file_type().is_file() || meta.len() == 0 || meta.len() > MAX_FILE_BYTES {
            return;
        }

        let mut buf = Vec::with_capacity(meta.len() as usize);
        if file.take(MAX_FILE_BYTES).read_to_end(&mut buf).is_err() {
            return;
        }
        let raw = String::from_utf8_lossy(&buf);

        for line in raw.lines() {
            if self.items.len() >= MAX_HISTORY_ITEMS {
                break;
            }
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let mut cols = line.split('\t');
            let (Some(k_s), Some(key_s), Some(name_s), Some(pct_s), Some(ts_s)) =
                (cols.next(), cols.next(), cols.next(), cols.next(), cols.next())
            else {
                continue;
            };
            if cols.next().is_some() {
                continue;
            }

            let kind = match k_s.parse::<u8>() {
                Ok(0) => DeviceKind::Mouse,
                Ok(1) => DeviceKind::Keyboard,
                Ok(2) => DeviceKind::Headphones,
                _ => continue,
            };
            let key = sanitize_device_text(key_s, 64);
            let name = sanitize_device_text(name_s, 64);
            let Ok(percent) = pct_s.parse::<i32>() else {
                continue;
            };
            let Ok(last_seen_unix) = ts_s.parse::<i64>() else {
                continue;
            };

            if key.is_empty() || !(0..=100).contains(&percent) || last_seen_unix <= 0 {
                continue;
            }
            self.items.push(HistoryItem { kind, key, name, percent, last_seen_unix });
        }
    }

    fn save(&self) {
        if self.demo_mode {
            return;
        }
        let (Some(dir), Some(path)) = (Self::storage_dir(), Self::storage_path()) else {
            return;
        };
        let _ = DirBuilder::new().recursive(true).mode(0o700).create(&dir);
        let Ok(dir_meta) = fs::symlink_metadata(&dir) else {
            return;
        };
        use std::os::unix::fs::MetadataExt;
        if !dir_meta.file_type().is_dir()
            || dir_meta.uid() != unsafe { crate::dbus_ffi::getuid() }
            || (dir_meta.mode() & 0o077) != 0
        {
            return;
        }

        let mut payload = String::with_capacity(self.items.len() * 80);
        for it in &self.items {
            let safe_key = sanitize_device_text(&it.key, 64);
            let safe_name = sanitize_device_text(&it.name, 64);
            if safe_key.is_empty() || !(0..=100).contains(&it.percent) {
                continue;
            }
            payload.push_str(&(it.kind as u8).to_string());
            payload.push('\t');
            payload.push_str(&safe_key);
            payload.push('\t');
            payload.push_str(&safe_name);
            payload.push('\t');
            payload.push_str(&it.percent.to_string());
            payload.push('\t');
            payload.push_str(&it.last_seen_unix.to_string());
            payload.push('\n');
        }

        let tmp_path = dir.join("history.tsv.tmp");
        let Ok(mut f) = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(O_NOFOLLOW | O_CLOEXEC)
            .open(&tmp_path)
        else {
            return;
        };
        if !f.metadata().map(|m| m.file_type().is_file()).unwrap_or(false) {
            drop(f);
            let _ = fs::remove_file(&tmp_path);
            return;
        }
        if f.write_all(payload.as_bytes()).is_err() {
            drop(f);
            let _ = fs::remove_file(&tmp_path);
            return;
        }
        drop(f);
        if fs::rename(&tmp_path, &path).is_err() {
            let _ = fs::remove_file(&tmp_path);
        }
    }

    pub fn sync_active(&mut self, active: &[DeviceState; 3]) {
        let now = now_unix_sec();
        let mut dirty = false;

        for (k, slot) in active.iter().enumerate() {
            let kind = match k {
                0 => DeviceKind::Mouse,
                1 => DeviceKind::Keyboard,
                _ => DeviceKind::Headphones,
            };
            for d in &slot.items {
                if d.key.is_empty() || !(0..=100).contains(&d.percent) {
                    continue;
                }
                let norm_name = d.name.to_lowercase();
                if let Some(existing) = self.items.iter_mut().find(|h| {
                    h.kind == kind && (h.key == d.key || (!norm_name.is_empty() && h.name.to_lowercase() == norm_name))
                }) {
                    if existing.percent != d.percent
                        || (!d.name.is_empty() && existing.name != d.name)
                        || existing.key != d.key
                    {
                        existing.key = d.key.clone();
                        if !d.name.is_empty() {
                            existing.name = d.name.clone();
                        }
                        existing.percent = d.percent;
                        existing.last_seen_unix = now;
                        dirty = true;
                    } else if now - existing.last_seen_unix >= TIMESTAMP_REFRESH_SEC {
                        existing.last_seen_unix = now;
                        dirty = true;
                    }
                } else {
                    self.items.push(HistoryItem {
                        kind,
                        key: d.key.clone(),
                        name: d.name.clone(),
                        percent: d.percent,
                        last_seen_unix: now,
                    });
                    dirty = true;
                }
            }
        }

        if self.items.len() > MAX_HISTORY_ITEMS {
            self.items.sort_by(|a, b| b.last_seen_unix.cmp(&a.last_seen_unix));
            self.items.truncate(MAX_HISTORY_ITEMS);
            dirty = true;
        }

        if dirty {
            self.save();
        }
    }

    pub fn get_inactive_for_kind(&self, kind: DeviceKind, active_items: &[DeviceEntry]) -> Vec<HistoryItem> {
        let mut out: Vec<HistoryItem> = self
            .items
            .iter()
            .filter(|it| it.kind == kind && !matches_active(it, active_items))
            .cloned()
            .collect();
        out.sort_by(|a, b| b.last_seen_unix.cmp(&a.last_seen_unix));
        out
    }

    pub fn remove_entry(&mut self, kind: DeviceKind, key: &str) -> bool {
        let before = self.items.len();
        self.items.retain(|it| !(it.kind == kind && it.key == key));
        if self.items.len() != before {
            self.save();
            true
        } else {
            false
        }
    }

    pub fn clear_inactive_for_kind(&mut self, kind: DeviceKind, active_items: &[DeviceEntry]) -> bool {
        let before = self.items.len();
        self.items.retain(|it| !(it.kind == kind && !matches_active(it, active_items)));
        if self.items.len() != before {
            self.save();
            true
        } else {
            false
        }
    }

    pub fn format_relative_time(last_seen_unix: i64) -> String {
        let now = now_unix_sec();
        if last_seen_unix <= 0 || now <= last_seen_unix {
            return "только что".to_string();
        }
        let diff = now - last_seen_unix;
        if diff < 60 {
            "только что".to_string()
        } else if diff < 3600 {
            format!("{} мин назад", diff / 60)
        } else if diff < 86400 {
            format!("{} ч назад", diff / 3600)
        } else if diff < 172_800 {
            "вчера".to_string()
        } else {
            format!("{} дн. назад", diff / 86400)
        }
    }
}