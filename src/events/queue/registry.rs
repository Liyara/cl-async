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

    pub fn push_event(&self, event: crate::events::Event) -> bool {
        if let Some(queue) = self.inner.registry.get(&event.key) {
            queue.push(event);
            true
        } else { false }
    }

    pub fn push_and_wake(&self, event: crate::events::Event) -> bool {
        if let Some(queue) = self.inner.registry.get(&event.key) {
            queue.push(event);
            queue.wake();
            true
        } else { false }
    }

    pub fn clear(&self) {
        self.inner.registry.clear();
    }

    pub fn broadcast(&self, event_type: crate::events::EventType) {
        for queue in self.inner.registry.iter() {
            let event = crate::events::Event {
                event_type,
                key: *queue.key(),
            };
            queue.value().push(event);
        }
    }

    pub fn broadcast_and_wake(&self, event_type: crate::events::EventType) {
        for queue in self.inner.registry.iter() {
            let event = crate::events::Event {
                event_type,
                key: *queue.key(),
            };
            queue.value().push(event);
            queue.value().wake();
        }
    }

    pub fn wake_all(&self) {
        for queue in self.inner.registry.iter() {
            queue.value().wake();
        }
    }

}