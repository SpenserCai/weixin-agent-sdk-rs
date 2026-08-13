//! Utility helpers: log redaction, ID generation, and transport diagnostics.

pub(crate) mod net_error;
pub mod random;
pub mod redact;

/// Current time in milliseconds since UNIX epoch.
#[allow(clippy::cast_possible_truncation)] // u128 → u64: won't overflow until year 584942417
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Convert a millisecond timestamp to the protocol's signed representation.
///
/// Saturates at `i64::MAX` instead of wrapping. Split out as a pure function so
/// the saturation behaviour is testable without reading the system clock.
pub(crate) fn ms_to_i64(ms: u64) -> i64 {
    i64::try_from(ms).unwrap_or(i64::MAX)
}

/// Current Unix time in milliseconds, typed for protocol timestamp fields.
pub(crate) fn now_ms_i64() -> i64 {
    ms_to_i64(now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ms_to_i64_saturates_instead_of_wrapping() {
        assert_eq!(ms_to_i64(0), 0);
        assert_eq!(ms_to_i64(1_700_000_000_000), 1_700_000_000_000);
        let max = u64::try_from(i64::MAX).unwrap();
        assert_eq!(ms_to_i64(max), i64::MAX);
        assert_eq!(ms_to_i64(max + 1), i64::MAX);
        assert_eq!(ms_to_i64(u64::MAX), i64::MAX);
        // Smoke check for the clock-reading wrapper; deliberately not compared
        // against a second clock read, which a wall-clock rollback could break.
        assert!(now_ms_i64() > 0);
    }
}
