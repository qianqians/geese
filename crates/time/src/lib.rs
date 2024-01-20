use std::sync::atomic::{AtomicI64, Ordering};
use chrono::prelude::*;

pub fn utc_unix_time() -> i64 {
    let utc: DateTime<Utc> = Utc::now();
    utc.timestamp_millis()
}

static mut OFFSET: AtomicI64 = AtomicI64::new(0);

pub fn set_time_offset(offset: i64) {
    unsafe {
        OFFSET.store(offset, Ordering::SeqCst)
    }
}

pub fn utc_unix_time_with_offset() -> i64 {
    unsafe {
        let offset = OFFSET.load(Ordering::SeqCst);
        utc_unix_time() + offset
    }
}