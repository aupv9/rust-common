use chrono::{DateTime, SubsecRound, Utc};

pub type Timestamp = DateTime<Utc>;

#[must_use]
pub fn now() -> Timestamp {
    Utc::now().trunc_subsecs(6)
}

#[cfg(test)]
mod tests {
    use chrono::Timelike;

    use super::*;

    #[test]
    fn now_has_microsecond_precision() {
        assert_eq!(now().nanosecond() % 1_000, 0);
    }

    #[test]
    fn serializes_as_rfc3339() {
        let ts: Timestamp = "2026-11-04T08:00:00.123456Z".parse().unwrap();
        assert_eq!(
            serde_json::to_string(&ts).unwrap(),
            "\"2026-11-04T08:00:00.123456Z\""
        );
    }
}
