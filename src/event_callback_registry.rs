use std::sync::Arc;
use dashmap::DashMap;
use log::error;
use thiserror::Error;
use crate::{event_poller::{EventRegistry, InterestType, Key, PollEvent}, task::Task};

#[derive(Debug, Error)]
pub enum EventCallbackRegistryError {
    #[error("Key not found")]
    KeyNotFound,

    #[error("Key already in use")]
    KeyAlreadyInUse,
}

pub struct EventCallbackData {
    key: Key,
    event_registry: EventRegistry,
}

impl EventCallbackData {

    pub fn modify(
        &self, 
        interest_type: InterestType
    ) {
        self.event_registry.modify_interest(self.key, interest_type).unwrap_or_else(
            |e| error!(
                "Error modifying interest {}: {}", 
                self.key.as_u64(),
                e
            )
        )
    }

    pub fn deregister(&self) {
        self.event_registry.deregister_interest(self.key).unwrap_or_else(
            |e| error!(
                "Error deregistering interest {}: {}",
                self.key.as_u64(),
                e
            )
        )
    }
    
}

pub trait EventHandler {
    fn handle(&self, event: PollEvent, data: EventCallbackData) -> Option<Task>;
}

pub type Callback = dyn EventHandler + Send + Sync;

pub struct EventCallbackRegistry {
    pub callbacks: DashMap<Key, Arc<Callback>>,
}

impl EventCallbackRegistry {
    pub fn new() -> Self {
        Self {
            callbacks: DashMap::new(),
        }
    }

    pub fn register<H>(
        &self, 
        key: Key, 
        handler: H,
    ) -> Result<(), EventCallbackRegistryError> 
    where
        H: EventHandler + Send + Sync + 'static,
    {
        match self.callbacks.entry(key) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                Err(EventCallbackRegistryError::KeyAlreadyInUse)
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
    ) -> Result<(), EventCallbackRegistryError> {
        match self.callbacks.remove(&key) {
            Some(_) => Ok(()),
            None => Err(EventCallbackRegistryError::KeyNotFound),
        }
    }

    pub fn update<H>(
        &self,
        key: Key,
        handler: H,
    ) -> Result<(), EventCallbackRegistryError> 
    where
        H: EventHandler + Send + Sync + 'static,
    {
        self.deregister(key)?;
        self.register(key, handler)
    }

    pub fn get(&self, key: Key) -> Option<Arc<Callback>> {
        self.callbacks.get(&key).map(|cb| Arc::clone(cb.value()))
    }

    pub fn try_run(
        &self, 
        key: Key, 
        event: PollEvent, 
        event_registry: EventRegistry
    ) -> Option<Task> {
        if let Some(cb) = self.callbacks.get(&key) {
            cb.value().handle(event, EventCallbackData { key, event_registry })
        } else { None }
    }

    pub fn run(
        &self,
        key: Key,
        event: PollEvent,
        event_registry: EventRegistry
    ) -> Result<Option<Task>, EventCallbackRegistryError> {
        let cb = self.get(key).ok_or(EventCallbackRegistryError::KeyNotFound)?;
        Ok(cb.handle(event, EventCallbackData { key, event_registry }))
    }
}