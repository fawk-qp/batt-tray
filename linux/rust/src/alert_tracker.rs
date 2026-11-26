#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertLevel {
    None = 0,
    Low15 = 1,
    Critical5 = 2,
}

struct Entry {
    slot: usize,
    name: String,
    level: AlertLevel,
    last_seen_sec: u64,
}

pub struct AlertTracker {
    entries: Vec<Entry>,
}

impl AlertTracker {
    const LOW_THRESHOLD: i32 = 15;
    const CRITICAL_THRESHOLD: i32 = 5;
    const RESET_THRESHOLD: i32 = 20;
    const OFFLINE_RESET_SEC: u64 = 1800;
    const MAX_ENTRIES: usize = 32;

    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(8),
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn prune_offline(&mut self, now_sec: u64) {
        self.entries
            .retain(|e| now_sec.saturating_sub(e.last_seen_sec) < Self::OFFLINE_RESET_SEC);
    }

    pub fn evaluate(
        &mut self,
        slot: usize,
        name: &str,
        percent: i32,
        charging: bool,
        now_sec: u64,
    ) -> AlertLevel {
        if !(0..=100).contains(&percent) || name.is_empty() {
            return AlertLevel::None;
        }

        let idx = self.find_or_create(slot, name, now_sec);
        let entry = &mut self.entries[idx];
        entry.last_seen_sec = now_sec;

        if charging || percent >= Self::RESET_THRESHOLD {
            entry.level = AlertLevel::None;
            return AlertLevel::None;
        }

        if percent <= Self::CRITICAL_THRESHOLD && entry.level < AlertLevel::Critical5 {
            entry.level = AlertLevel::Critical5;
            return AlertLevel::Critical5;
        }
        if percent <= Self::LOW_THRESHOLD && entry.level < AlertLevel::Low15 {
            entry.level = AlertLevel::Low15;
            return AlertLevel::Low15;
        }
        AlertLevel::None
    }

    fn find_or_create(&mut self, slot: usize, name: &str, now_sec: u64) -> usize {
        if let Some(pos) = self
            .entries
            .iter()
            .position(|e| e.slot == slot && e.name == name)
        {
            return pos;
        }
        if self.entries.len() >= Self::MAX_ENTRIES {
            let oldest = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.last_seen_sec)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.entries[oldest] = Entry {
                slot,
                name: name.to_string(),
                level: AlertLevel::None,
                last_seen_sec: now_sec,
            };
            return oldest;
        }
        self.entries.push(Entry {
            slot,
            name: name.to_string(),
            level: AlertLevel::None,
            last_seen_sec: now_sec,
        });
        self.entries.len() - 1
    }
}
