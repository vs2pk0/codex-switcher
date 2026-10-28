use chrono::{DateTime, Duration, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

pub const SYSTEM_TIME_ZONE: &str = "system";

pub fn normalize_time_zone(value: &str) -> String {
    let value = value.trim();
    if value.parse::<chrono_tz::Tz>().is_ok() {
        value.to_string()
    } else {
        SYSTEM_TIME_ZONE.to_string()
    }
}

#[derive(Clone, Copy)]
pub struct AppTimeZone(Option<chrono_tz::Tz>);

impl AppTimeZone {
    pub fn new(value: &str) -> Self {
        Self(value.parse().ok())
    }

    pub fn current() -> Self {
        Self::new(
            &crate::read_switcher_settings()
                .map(|settings| settings.time_zone)
                .unwrap_or_default(),
        )
    }

    pub fn now(self) -> DateTime<FixedOffset> {
        self.convert(Utc::now())
    }

    pub fn convert(self, value: DateTime<Utc>) -> DateTime<FixedOffset> {
        match self.0 {
            Some(zone) => value.with_timezone(&zone).fixed_offset(),
            None => value.with_timezone(&Local).fixed_offset(),
        }
    }

    pub fn at(self, seconds: i64) -> Option<DateTime<FixedOffset>> {
        DateTime::from_timestamp(seconds, 0).map(|value| self.convert(value))
    }

    pub fn from_local(self, value: &NaiveDateTime) -> Option<DateTime<FixedOffset>> {
        match self.0 {
            Some(zone) => zone
                .from_local_datetime(value)
                .earliest()
                .map(|value| value.fixed_offset()),
            None => Local
                .from_local_datetime(value)
                .earliest()
                .map(|value| value.fixed_offset()),
        }
    }

    pub fn day_start(self, date: NaiveDate) -> i64 {
        let midnight = date.and_hms_opt(0, 0, 0).expect("valid midnight");
        // Midnight itself can be skipped by a DST transition.
        (0..1440)
            .find_map(|minute| self.from_local(&(midnight + Duration::minutes(minute))))
            .map(|value| value.timestamp())
            .unwrap_or_else(|| midnight.and_utc().timestamp())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_preferences_and_defaults_old_settings_to_system() {
        assert_eq!(normalize_time_zone("Asia/Shanghai"), "Asia/Shanghai");
        assert_eq!(normalize_time_zone("UTC"), "UTC");
        for value in ["", "system", "Mars/Olympus", "GMT+8"] {
            assert_eq!(normalize_time_zone(value), SYSTEM_TIME_ZONE);
        }
        assert_eq!(
            crate::CodexSwitcherSettings::default().time_zone,
            SYSTEM_TIME_ZONE
        );
        let mut legacy = serde_json::to_value(crate::CodexSwitcherSettings::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("timeZone");
        let restored: crate::CodexSwitcherSettings = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.time_zone, SYSTEM_TIME_ZONE);
    }

    #[test]
    fn date_boundaries_respect_fractional_offsets_and_dst() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
        let zone = AppTimeZone::new("America/Los_Angeles");
        assert_eq!(
            zone.day_start(date.succ_opt().unwrap()) - zone.day_start(date),
            23 * 3600
        );
        let date = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        assert_eq!(
            zone.day_start(date.succ_opt().unwrap()) - zone.day_start(date),
            25 * 3600
        );
        let kathmandu = AppTimeZone::new("Asia/Kathmandu");
        assert_eq!(
            kathmandu.at(0).unwrap().format("%H:%M").to_string(),
            "05:30"
        );
        assert_eq!(
            kathmandu.now().offset().local_minus_utc(),
            5 * 3600 + 45 * 60
        );
    }

    #[test]
    fn nonexistent_local_time_is_rejected_and_repeated_time_uses_first_occurrence() {
        let zone = AppTimeZone::new("America/Los_Angeles");
        let gap = NaiveDate::from_ymd_opt(2026, 3, 8)
            .unwrap()
            .and_hms_opt(2, 30, 0)
            .unwrap();
        assert!(zone.from_local(&gap).is_none());
        let repeat = NaiveDate::from_ymd_opt(2026, 11, 1)
            .unwrap()
            .and_hms_opt(1, 30, 0)
            .unwrap();
        assert_eq!(
            zone.from_local(&repeat).unwrap().offset().local_minus_utc(),
            -7 * 3600
        );
    }
}
