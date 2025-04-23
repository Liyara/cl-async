mod r#type;

pub (crate) mod poller;
pub (crate) mod channel;

mod source;
mod queue;
mod receiver;

pub mod futures;

pub use r#type::EventType;
pub use channel::EventChannel;
pub use channel::EventChannelSender;
pub use channel::EventChannelReceiver;
pub use poller::EventPoller;
pub use poller::InterestType;
pub use source::EventSource;
pub use queue::EventQueue;
pub use queue::registry::EventQueueRegistry;
pub use receiver::EventReceiver;

use crate::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub event_type: EventType,
    pub key: Key,
}

impl Event {

    pub (in crate::events) fn from_libc_epoll_event(event: &libc::epoll_event) -> Self {
        Self {
            event_type: EventType::from_bits_truncate(event.events as i32),
            key: Key::new(event.u64),
        }
    }

    pub fn is_error(&self) -> bool {
        self.event_type.contains(EventType::ERROR)
    }

    pub fn is_readable(&self) -> bool {
        self.event_type.contains(EventType::READABLE)
    }

    pub fn is_writable(&self) -> bool {
        self.event_type.contains(EventType::WRITABLE)
    }

    pub fn is_hup(&self) -> bool {
        self.event_type.contains(EventType::HUP)
    }

    pub fn is_rdhup(&self) -> bool {
        self.event_type.contains(EventType::RDHUP)
    }

    pub fn is_shutdown(&self) -> bool {
        self.event_type.contains(EventType::SHUTDOWN)
    }

    pub fn is_kill(&self) -> bool {
        self.event_type.contains(EventType::KILL)
    }
}