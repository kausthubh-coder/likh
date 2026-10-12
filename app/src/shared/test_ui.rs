//! iced_test simulators crash the test process (heap corruption or access
//! violation, about 1 run in 5 on Windows) when two run at once on different
//! threads. Every view test holds this lock while it uses a simulator, so the
//! rest of the suite can still run in parallel.

use std::sync::{Mutex, MutexGuard};

static UI: Mutex<()> = Mutex::new(());

pub fn lock() -> MutexGuard<'static, ()> {
    // A panicking view test poisons the lock; the next test can still run.
    UI.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
