use std::sync::{
    atomic::{
        AtomicU64, 
        Ordering
    }, 
    Arc
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key(u64);

impl Key {
    pub fn new(val: u64) -> Self {
        Self(val)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct KeyGenerator {
    inner: Arc<AtomicU64>
}

impl KeyGenerator {
    pub fn new(min_key_value: u64) -> Self {
        Self {
            inner: Arc::new(AtomicU64::new(min_key_value)),
        }
    }

    pub fn get(&self) -> Key {
        Key::new(self.inner.fetch_add(1, Ordering::Relaxed))
    }
}