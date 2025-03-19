use std::sync::Arc;
use dashmap::DashMap;
use thiserror::Error;
use crate::{
    events::{poller::EventPollerRegistry, SubscriptionHandle}, 
    Event, 
    Key, Task
};
use super::EventHandler;

#[derive(Debug, Error)]
pub enum EventHandlerRegistryError {
    #[error("Key not found")]
    KeyNotFound,

    #[error("Key already in use")]
    KeyAlreadyInUse,
}


pub struct EventHandlerRegistry {
    pub handlers: DashMap<Key, Arc<dyn EventHandler + Send + Sync>>,
}

impl EventHandlerRegistry {
    pub fn new() -> Self {
        Self {
            handlers: DashMap::new(),
        }
    }

    pub fn register<H>(
        &self, 
        key: Key, 
        handler: H,
    ) -> Result<(), EventHandlerRegistryError> 
    where
        H: EventHandler + Send + Sync + 'static,
    {
        match self.handlers.entry(key) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                Err(EventHandlerRegistryError::KeyAlreadyInUse)
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(Arc::new(handler));
                Ok(())
            }
        }
    }

    pub fn deregister(
        &self,
        key: Key
    ) -> Result<(), EventHandlerRegistryError> {
        match self.handlers.remove(&key) {
            Some(_) => Ok(()),
            None => Err(EventHandlerRegistryError::KeyNotFound),
        }
    }

    pub fn update<H>(
        &self,
        key: Key,
        handler: H,
    ) -> Result<(), EventHandlerRegistryError> 
    where
        H: EventHandler + Send + Sync + 'static,
    {
        self.deregister(key)?;
        self.register(key, handler)
    }

    pub fn get(&self, key: Key) -> Option<Arc<dyn EventHandler + Send + Sync>> {
        self.handlers.get(&key).map(|cb| Arc::clone(cb.value()))
    }

    pub fn run(
        self: &Arc<Self>,
        key: Key,
        event: Event,
        event_registry: EventPollerRegistry
    ) -> Result<Option<Task>, EventHandlerRegistryError> {
        let cb = self.get(key).ok_or(EventHandlerRegistryError::KeyNotFound)?;
        Ok(cb.handle(
            event, 
            SubscriptionHandle::new(
                key, 
                event_registry, 
                Arc::clone(&self)
            )
        ))
    }
}