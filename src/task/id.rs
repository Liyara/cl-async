use std::{fmt, sync::atomic::{AtomicU64, Ordering}};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(u64);

impl TaskId {
    pub fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        Self(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}


impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}