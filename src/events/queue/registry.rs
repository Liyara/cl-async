use std::sync::Arc;

use dashmap::DashMap;

use crate::{
    events::EventQueue, 
    Key
};

#[derive(Debug)]
struct EventQueueRegistryInner {
    registry: DashMap<Key, EventQueue>
}

#[derive(Debug, Clone)]
pub struct EventQueueRegistry {
    inner: Arc<EventQueueRegistryInner>,
}

impl EventQueueRegistry {

    pub fn new() -> Self {
        Self {
            inner: Arc::new(EventQueueRegistryInner {
                registry: DashMap::new()
            })
        }
    }

    pub fn register(&self, key: Key, queue: EventQueue) {
        self.inner.registry.insert(key, queue);
    }

    pub fn wake(&self, key: Key) {
        if let Some(queue) = self.inner.registry.get(&key) {
            queue.wake();
        }
    }

    pub fn remove(&self, key: Key) {
        self.inner.registry.remove(&key);
    }

    pub fn is_empty(&self) -> bool {
        self.inner.registry.is_empty()
    }

    pub fn keys(&self) -> Vec<Key> {
        self.inner.registry.iter().map(|v| *v.key()).collect()
    }

    pub fn push_event(&self, key: Key, event: crate::events::Event) -> bool {
        if let Some(queue) = self.inner.registry.get(&key) {
            queue.push(event);
            true
        } else { false }
    }

}