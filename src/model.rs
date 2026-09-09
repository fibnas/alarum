use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, TimeZone, Timelike, Weekday};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    Once,
    Daily,
    Weekdays,
    Weekends,
}

impl Repeat {
    pub fn label(self) -> &'static str {
        match self {
            Repeat::Once => "Once",
            Repeat::Daily => "Every day",
            Repeat::Weekdays => "Weekdays",
            Repeat::Weekends => "Weekends",
        }
    }

    pub fn matches(self, weekday: Weekday) -> bool {
        match self {
            Repeat::Once | Repeat::Daily => true,
            Repeat::Weekdays => !matches!(weekday, Weekday::Sat | Weekday::Sun),
            Repeat::Weekends => matches!(weekday, Weekday::Sat | Weekday::Sun),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    pub id: Uuid,
    pub enabled: bool,
    pub hour: u32,
    pub minute: u32,
    pub label: String,
    /// Empty string means "use the built-in default sound".
    pub sound_path: String,
    pub snooze_minutes: u32,
    pub repeat: Repeat,
    /// When set, this one-off time overrides the regular schedule (used for snooze).
    pub snooze_until: Option<DateTime<Local>>,
    pub last_fired: Option<DateTime<Local>>,
}

impl Alarm {
    pub fn new(hour: u32, minute: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            enabled: true,
            hour,
            minute,
            label: String::new(),
            sound_path: String::new(),
            snooze_minutes: 5,
            repeat: Repeat::Once,
            snooze_until: None,
            last_fired: None,
        }
    }

    pub fn time_label_for(&self, use_12_hour: bool) -> String {
        format_hm(self.hour, self.minute, use_12_hour)
    }

    pub fn display_name(&self) -> String {
        let trimmed = self.label.trim();
        if trimmed.is_empty() {
            "Alarm".to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Next time this alarm should ring, if enabled.
    pub fn next_trigger(&self, now: DateTime<Local>) -> Option<DateTime<Local>> {
        if !self.enabled {
            return None;
        }

        if let Some(until) = self.snooze_until {
            if until > now {
                return Some(until);
            }
        }

        // Look up to 8 days ahead so weekend/weekday rules resolve.
        for day_offset in 0..8 {
            let candidate_date = now.date_naive() + Duration::days(day_offset);
            let weekday = candidate_date.weekday();
            if !self.repeat.matches(weekday) {
                continue;
            }
            let naive = candidate_date.and_time(NaiveTime::from_hms_opt(self.hour, self.minute, 0)?);
            if let Some(candidate) = Local.from_local_datetime(&naive).earliest() {
                // Same-minute already handled this cycle? Skip to next occurrence.
                let already = self
                    .last_fired
                    .map(|fired| fired >= candidate && fired.date_naive() == candidate.date_naive())
                    .unwrap_or(false);
                if candidate > now && !already {
                    return Some(candidate);
                }
                if candidate <= now && day_offset == 0 && !already && self.repeat == Repeat::Once {
                    // A once-alarm whose time is still "now" (same minute) should fire.
                    if now.hour() == self.hour && now.minute() == self.minute {
                        return Some(candidate);
                    }
                }
                if candidate <= now {
                    continue;
                }
                return Some(candidate);
            }
        }
        None
    }

    pub fn should_fire(&self, now: DateTime<Local>) -> bool {
        if !self.enabled {
            return false;
        }

        if let Some(until) = self.snooze_until {
            if now >= until {
                let already = self.last_fired.map(|f| f >= until).unwrap_or(false);
                return !already;
            }
            return false;
        }

        if !self.repeat.matches(now.weekday()) {
            return false;
        }

        if now.hour() != self.hour || now.minute() != self.minute {
            return false;
        }

        match self.last_fired {
            None => true,
            Some(fired) => fired.date_naive() != now.date_naive() || fired.hour() != now.hour() || fired.minute() != now.minute(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AlarmStore {
    pub alarms: Vec<Alarm>,
    pub default_snooze_minutes: u32,
    /// When true, clocks and editors use 12-hour time with AM/PM.
    #[serde(default)]
    pub use_12_hour: bool,
}

impl AlarmStore {
    pub fn new() -> Self {
        Self {
            alarms: Vec::new(),
            default_snooze_minutes: 5,
            use_12_hour: false,
        }
    }
}

/// Convert a 0–23 hour to 1–12 plus whether it is PM.
pub fn to_12_hour(hour24: u32) -> (u32, bool) {
    let is_pm = hour24 >= 12;
    let hour12 = hour24 % 12;
    (if hour12 == 0 { 12 } else { hour12 }, is_pm)
}

/// Convert a 1–12 hour plus AM/PM to 0–23.
pub fn to_24_hour(hour12: u32, is_pm: bool) -> u32 {
    let base = if hour12 == 12 { 0 } else { hour12 % 12 };
    if is_pm { base + 12 } else { base }
}

pub fn format_hm(hour: u32, minute: u32, use_12_hour: bool) -> String {
    if use_12_hour {
        let (h, is_pm) = to_12_hour(hour);
        format!("{}:{:02} {}", h, minute, if is_pm { "PM" } else { "AM" })
    } else {
        format!("{:02}:{:02}", hour, minute)
    }
}

pub fn format_datetime(dt: DateTime<Local>, use_12_hour: bool) -> String {
    if use_12_hour {
        dt.format("%a %b %d   %I:%M:%S %p").to_string()
    } else {
        dt.format("%a %b %d   %H:%M:%S").to_string()
    }
}

pub fn format_next(dt: DateTime<Local>, use_12_hour: bool) -> String {
    if use_12_hour {
        dt.format("%a %I:%M %p").to_string()
    } else {
        dt.format("%a %H:%M").to_string()
    }
}
