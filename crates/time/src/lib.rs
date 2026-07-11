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
}
