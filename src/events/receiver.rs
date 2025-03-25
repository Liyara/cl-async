use super::{
    futures::NextEventFuture,
     poller::{
        registry::EventPollerRegistryError, 
        EventPollerRegistry
    }, 
    EventQueue, 
    EventSource, 
    InterestType
};


pub struct EventReceiver {
    source: EventSource,
    key: crate::Key,
    queue: EventQueue,
    poller_registry: EventPollerRegistry,
}

impl EventReceiver {
    pub fn new(
        source: EventSource,
        key: crate::Key,
        queue: EventQueue,
        poller_registry: EventPollerRegistry,
    ) -> Self {
        Self {
            source,
            key,
            queue,
            poller_registry,
        }
    }

    pub fn source(&self) -> EventSource {
        self.source
    }

    pub fn next_event<'a, 'b>(&'a self) -> NextEventFuture<'b> 
    where 'a: 'b {
        self.queue.next_event()
    }

    pub fn modify_interest(&self, interest_type: InterestType) -> Result<(), EventPollerRegistryError> {
        self.poller_registry.modify_interest(self.key, interest_type)
    }
}