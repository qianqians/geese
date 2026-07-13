use std::sync::atomic::{AtomicI64, Ordering};
use chrono::prelude::*;

pub struct OffsetTime {
    off_set: AtomicI64
}

impl OffsetTime {
    pub fn new() -> OffsetTime {
        OffsetTime {
            off_set: AtomicI64::new(0)
        }
    }

    fn utc_unix_time(&self) -> i64 {
        let utc: DateTime<Utc> = Utc::now();
        utc.timestamp_millis()
    }

    pub fn set_time_offset(&self, offset: i64) {
        self.off_set.store(offset, Ordering::SeqCst)
    }

    pub fn utc_unix_time_with_offset(&self) -> i64 {
        let offset = self.off_set.load(Ordering::SeqCst);
        self.utc_unix_time() + offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn new_has_zero_offset() {
        let t = OffsetTime::new();
        assert_eq!(t.off_set.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn set_time_offset_stores_value() {
        let t = OffsetTime::new();
        t.set_time_offset(12345);
        assert_eq!(t.off_set.load(Ordering::SeqCst), 12345);
        t.set_time_offset(-9999);
        assert_eq!(t.off_set.load(Ordering::SeqCst), -9999);
    }

    #[test]
    fn utc_unix_time_with_offset_returns_positive() {
        let t = OffsetTime::new();
        assert!(t.utc_unix_time_with_offset() > 0);
    }

    #[test]
    fn positive_offset_increases_time() {
        let t = OffsetTime::new();
        let before = t.utc_unix_time_with_offset();
        t.set_time_offset(10_000_000); // +10 seconds
        let after = t.utc_unix_time_with_offset();
        assert!(after - before >= 10_000_000);
    }

    #[test]
    fn consecutive_calls_are_monotonic() {
        let t = OffsetTime::new();
        let t1 = t.utc_unix_time_with_offset();
        thread::sleep(Duration::from_millis(5));
        let t2 = t.utc_unix_time_with_offset();
        thread::sleep(Duration::from_millis(5));
        let t3 = t.utc_unix_time_with_offset();
        assert!(t2 >= t1);
        assert!(t3 >= t2);
    }

    #[test]
    fn negative_offset_decreases_time() {
        let t = OffsetTime::new();
        let base = t.utc_unix_time_with_offset();
        t.set_time_offset(-10_000_000); // -10 seconds
        let adjusted = t.utc_unix_time_with_offset();
        // adjusted should be approximately base - 10_000_000 (within some tolerance for elapsed time)
        let diff = base - adjusted;
        assert!(diff >= 9_999_000 && diff <= 10_100_000,
            "expected ~10_000_000ms difference, got {}", diff);
    }

    #[test]
    fn set_offset_overwrites_previous() {
        let t = OffsetTime::new();
        t.set_time_offset(100);
        assert_eq!(t.off_set.load(Ordering::SeqCst), 100);
        t.set_time_offset(200);
        assert_eq!(t.off_set.load(Ordering::SeqCst), 200);
        t.set_time_offset(0);
        assert_eq!(t.off_set.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn utc_unix_time_is_positive() {
        let t = OffsetTime::new();
        let ts = t.utc_unix_time();
        // Current UTC timestamp should be well above 0 (post-1970)
        assert!(ts > 1_000_000_000_000, "timestamp should be in milliseconds since epoch, got {}", ts);
    }

    #[test]
    fn zero_offset_returns_same_as_raw() {
        let t = OffsetTime::new();
        // With zero offset, utc_unix_time_with_offset should be close to utc_unix_time
        let raw = t.utc_unix_time();
        let with_offset = t.utc_unix_time_with_offset();
        // They should be within a few milliseconds of each other
        let diff = (with_offset - raw).abs();
        assert!(diff < 100, "expected <100ms difference, got {}ms", diff);
    }

    #[test]
    fn large_offset_values() {
        let t = OffsetTime::new();
        // Test with very large offset (1 year in milliseconds)
        let one_year_ms: i64 = 365 * 24 * 60 * 60 * 1000;
        t.set_time_offset(one_year_ms);
        let result = t.utc_unix_time_with_offset();
        let raw = t.utc_unix_time();
        let diff = result - raw;
        assert!((diff - one_year_ms).abs() < 100,
            "expected ~{} difference, got {}", one_year_ms, diff);
    }
}
