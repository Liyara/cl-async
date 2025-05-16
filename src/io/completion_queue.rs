use std::{
    sync::Arc, 
    task::Poll
};

use dashmap::DashMap;

use crate::Key;

use super::{
    IoCompletionResult, 
    IoCompletionState, 
    IoEntry
};

struct IoCompletionSlot {

    // State will only become None if an invalid state operation is attempted
    state: Option<IoCompletionState>,
    waker: Option<std::task::Waker>,
}

impl IoCompletionSlot {

    pub fn new(entry: IoEntry, waker: Option<std::task::Waker>) -> Self {
        Self {
            state: Some(IoCompletionState::NotCompleted(entry)),
            waker: waker
        }
    }

    pub fn is_valid_state(&self) -> bool {
        self.state.is_some()
    }

    pub fn update_waker(&mut self, waker: &std::task::Waker) {
        self.waker = Some(waker.clone());
    }

    // Returns true if the slot is still valid after the completion
    // Returns false if the slot is no longer valid
    pub fn complete(&mut self, result: i32) -> bool {
        let res = if let Some(state) = self.state.take() {
            if let Some(new_state) = state.complete(result) {
                self.state = Some(new_state);
                true
            } else { false }
        } else { false };

        if let Some(waker) = &self.waker {
            waker.wake_by_ref();
        }

        res
    }

    pub fn try_extract_result(&mut self) -> Result<Option<IoCompletionResult>, ()> {
        if let Some(state) = self.state.take() {
            let (
                result,
                new_state
            ) = state.into_result();
            self.state = new_state;
            Ok(result)
        } else { Err(()) }
    }
}

#[derive(Clone)]
pub struct IoCompletionQueue {
    inner: Arc<DashMap<Key, IoCompletionSlot>>,
}

impl IoCompletionQueue {

    pub (super) fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    pub fn poll(
        &self, 
        key: Key,
        cx: &mut std::task::Context<'_>
    ) -> Poll<Option<IoCompletionResult>> {
        let guard = self.inner.get_mut(&key);

        let mut entry = match guard {
            Some(entry) => entry,
            None => return Poll::Ready(None)
        };

        let slot = entry.value_mut();

        let result = slot.try_extract_result();
        let is_valid = slot.is_valid_state();

        let result = if let Err(_) = result {
            Poll::Ready(None)
        } else if let Some(result) = result.unwrap() {
            Poll::Ready(Some(result))
        } else {
            slot.update_waker(cx.waker());
            Poll::Pending
        };

        drop(entry);

        if !is_valid { self.inner.remove(&key); }

        result
    }

    pub fn add_entry(
        &mut self, 
        key: Key, 
        entry: IoEntry,
        waker: Option<std::task::Waker>
    ) {
        self.inner.insert(key, IoCompletionSlot::new(entry, waker));
    }

    // Returns true if the slot was removed
    pub fn complete(&mut self, key: Key, result: i32) -> bool {
        if let Some(mut slot) = self.inner.get_mut(&key) {
            let slot_value = slot.value_mut();
            if !slot_value.complete(result) {
                drop(slot);
                self.inner.remove(&key);
                true
            } else { false }
        } else { false }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}