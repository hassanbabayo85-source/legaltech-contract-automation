//! Reminder-date arithmetic.

use chrono::{DateTime, Days, NaiveDate, Utc};

use crate::reminders::types::ReminderType;

pub const STANDARD_OFFSETS: &[(u64, ReminderType)] = &[
    (7, ReminderType::SevenDaysBefore),
    (3, ReminderType::ThreeDaysBefore),
    (1, ReminderType::OneDayBefore),
    (0, ReminderType::OnDeadline),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputedReminder {
    pub reminder_date: NaiveDate,
    pub reminder_type: ReminderType,
}

pub fn date_to_utc_midnight(date: NaiveDate) -> DateTime<Utc> {
    let ndt = date
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is always a valid time");
    DateTime::from_naive_utc_and_offset(ndt, Utc)
}

pub fn system_reminders_for(deadline: NaiveDate, today_utc: NaiveDate) -> Vec<ComputedReminder> {
    STANDARD_OFFSETS
        .iter()
        .filter_map(|(days, kind)| {
            let reminder_date = deadline.checked_sub_days(Days::new(*days))?;
            if reminder_date < today_utc {
                None
            } else {
                Some(ComputedReminder {
                    reminder_date,
                    reminder_type: *kind,
                })
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn date_to_utc_midnight_is_deterministic() {
        let x = date_to_utc_midnight(d(2026, 10, 20));
        assert_eq!(x.to_rfc3339(), "2026-10-20T00:00:00+00:00");
    }

    #[test]
    fn full_schedule_when_deadline_is_far() {
        let today = d(2026, 1, 1);
        let deadline = d(2026, 10, 20);
        let got = system_reminders_for(deadline, today);
        assert_eq!(got.len(), 4);
        assert_eq!(got[0].reminder_date, d(2026, 10, 13));
        assert_eq!(got[0].reminder_type, ReminderType::SevenDaysBefore);
        assert_eq!(got[1].reminder_date, d(2026, 10, 17));
        assert_eq!(got[2].reminder_date, d(2026, 10, 19));
        assert_eq!(got[3].reminder_date, d(2026, 10, 20));
        assert_eq!(got[3].reminder_type, ReminderType::OnDeadline);
    }

    #[test]
    fn skips_offsets_already_past() {
        // Today is the day before the deadline. Only offsets that
        // fall on today or later survive: 1_day_before (today) and
        // on_deadline (tomorrow). 7d and 3d are already past.
        let today = d(2026, 10, 19);
        let deadline = d(2026, 10, 20);
        let got = system_reminders_for(deadline, today);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].reminder_type, ReminderType::OneDayBefore);
        assert_eq!(got[0].reminder_date, today);
        assert_eq!(got[1].reminder_type, ReminderType::OnDeadline);
        assert_eq!(got[1].reminder_date, deadline);
    }

    #[test]
    fn deadline_today_only_on_deadline() {
        let today = d(2026, 10, 20);
        let got = system_reminders_for(today, today);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].reminder_date, today);
        assert_eq!(got[0].reminder_type, ReminderType::OnDeadline);
    }

    #[test]
    fn deadline_yesterday_nothing() {
        let today = d(2026, 10, 20);
        let deadline = d(2026, 10, 19);
        assert!(system_reminders_for(deadline, today).is_empty());
    }

    #[test]
    fn deadline_in_two_days() {
        let today = d(2026, 10, 18);
        let deadline = d(2026, 10, 20);
        let got = system_reminders_for(deadline, today);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].reminder_date, d(2026, 10, 19));
        assert_eq!(got[1].reminder_date, d(2026, 10, 20));
    }

    #[test]
    fn deadline_in_three_days() {
        let today = d(2026, 10, 17);
        let deadline = d(2026, 10, 20);
        let got = system_reminders_for(deadline, today);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].reminder_type, ReminderType::ThreeDaysBefore);
    }

    #[test]
    fn deadline_in_seven_days() {
        let today = d(2026, 10, 13);
        let deadline = d(2026, 10, 20);
        let got = system_reminders_for(deadline, today);
        assert_eq!(got.len(), 4);
        assert_eq!(got[0].reminder_date, today);
        assert_eq!(got[0].reminder_type, ReminderType::SevenDaysBefore);
    }
}
