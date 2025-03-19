use std::sync::Arc;

use thiserror::Error;
use crate::Key;

use super::{
    handler::registry::{
        EventHandlerRegistry, 
        EventHandlerRegistryError
    }, 
    poller::{
        registry::EventPollerRegistryError, 
        EventPollerRegistry, 
        InterestType
    }
};


#[derive(Debug, Error)]
pub enum SubscriptionHandleError {
    #[error("Event Poller Registry error: {0}")]
    EventRegistry(#[from] EventPollerRegistryError),

    #[error("Event Handler Registry error: {0}")]
    EventHandlerRegistry(#[from] EventHandlerRegistryError),
}

pub struct SubscriptionHandle {
    key: Key,
    event_registry: EventPollerRegistry,
    handler_registry: Arc<EventHandlerRegistry>,
}

impl SubscriptionHandle {

    pub fn new(
        key: Key,
        event_registry: EventPollerRegistry,
        handler_registry: Arc<EventHandlerRegistry>,
    ) -> Self {
        Self {
            key,
            event_registry,
            handler_registry,
        }
    }

    pub fn modify(
        &self, 
        interest_type: InterestType
    ) -> Result<(), SubscriptionHandleError> {
        Ok(self.event_registry.modify_interest(self.key, interest_type)?)
    }

    pub fn deregister(&self) -> Result<(), SubscriptionHandleError> {
        self.event_registry.deregister_interest(self.key)?;
        self.handler_registry.deregister(self.key)?;
        Ok(())
    }
    
}