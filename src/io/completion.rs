use std::sync::Arc;

use dashmap::DashMap;

use crate::Key;


#[derive(Clone)]
pub struct IOCompletionQueue {
    inner: Arc<DashMap<Key, Result<IOCompletion, i32>>>,
}

impl IOCompletionQueue {

    pub (super) fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    pub (super) fn push(&self, key: Key, completion: Result<IOCompletion, i32>) {
        self.inner.insert(key, completion);
    }

    pub fn pop(&self, key: Key) -> Option<Result<IOCompletion, i32>> {
        self.inner.remove(&key).map(|(_, v)| v)
    }

    pub fn peek(&self, key: Key) -> bool {
        self.inner.contains_key(&key)
    }
}


pub struct IOReadCompletion {
    pub buffer: Vec<u8>,
}


pub struct IOWriteCompletion {
    pub bytes_written: usize,
}


pub enum IOCompletion {
    Read(IOReadCompletion),
    Write(IOWriteCompletion),
}