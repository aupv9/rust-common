//! Time helpers.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch.
///
/// # Panics
///
/// Panics if the system clock is set before 1970-01-01.
#[must_use]
pub fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_after_2020() {
        assert!(unix_millis() > 1_577_836_800_000);
    }
}
