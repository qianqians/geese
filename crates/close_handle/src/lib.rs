use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct CloseHandle {
    is_close: Arc<AtomicBool>
}

impl CloseHandle {
    pub fn new() -> CloseHandle {
        let term = Arc::new(AtomicBool::new(false));
        let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, term.clone());
        CloseHandle {
            is_close: term
        }
    }

    pub fn is_closed(&self) -> bool {
        return self.is_close.load(Ordering::Relaxed)
    }

    pub fn close(&mut self) {
        self.is_close.store(true, Ordering::SeqCst);
    }
}