//! Strongly typed reminders vocabulary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderType {
    SevenDaysBefore,
    ThreeDaysBefore,
    OneDayBefore,
    OnDeadline,
    Custom,
}

impl ReminderType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SevenDaysBefore => "7_days_before",
            Self::ThreeDaysBefore => "3_days_before",
            Self::OneDayBefore => "1_day_before",
            Self::OnDeadline => "on_deadline",
            Self::Custom => "custom",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "7_days_before" => Some(Self::SevenDaysBefore),
            "3_days_before" => Some(Self::ThreeDaysBefore),
            "1_day_before" => Some(Self::OneDayBefore),
            "on_deadline" => Some(Self::OnDeadline),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelType {
    Telegram,
    Discord,
    Webhook,
}

impl ChannelType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Discord => "discord",
            Self::Webhook => "webhook",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "telegram" => Some(Self::Telegram),
            "discord" => Some(Self::Discord),
            "webhook" => Some(Self::Webhook),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderSource {
    System,
    Custom,
}

impl ReminderSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderStatus {
    Pending,
    Processing,
    Sent,
    Failed,
    Cancelled,
}

impl ReminderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Sent => "sent",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "processing" => Some(Self::Processing),
            "sent" => Some(Self::Sent),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminder_type_round_trips() {
        for t in [
            ReminderType::SevenDaysBefore,
            ReminderType::ThreeDaysBefore,
            ReminderType::OneDayBefore,
            ReminderType::OnDeadline,
            ReminderType::Custom,
        ] {
            assert_eq!(ReminderType::parse(t.as_str()), Some(t));
        }
        assert_eq!(ReminderType::parse("bogus"), None);
    }

    #[test]
    fn channel_type_round_trips() {
        for c in [
            ChannelType::Telegram,
            ChannelType::Discord,
            ChannelType::Webhook,
        ] {
            assert_eq!(ChannelType::parse(c.as_str()), Some(c));
        }
        assert_eq!(ChannelType::parse("carrier_pigeon"), None);
    }

    #[test]
    fn status_round_trips() {
        for s in [
            ReminderStatus::Pending,
            ReminderStatus::Processing,
            ReminderStatus::Sent,
            ReminderStatus::Failed,
            ReminderStatus::Cancelled,
        ] {
            assert_eq!(ReminderStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(ReminderStatus::parse("bogus"), None);
    }
}
