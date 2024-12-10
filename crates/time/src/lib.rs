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

    pub fn set_time_offset(&mut self, offset: i64) {
        self.off_set.store(offset, Ordering::SeqCst)
    }

    pub fn utc_unix_time_with_offset(&self) -> i64 {
        let offset = self.off_set.load(Ordering::SeqCst);
        self.utc_unix_time() + offset
    }
}

