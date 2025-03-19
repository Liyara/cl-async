mod r#type;

pub mod handler;
pub mod poller;
pub mod channel;
pub mod subscription_handle;

pub use r#type::EventType;
pub use subscription_handle::SubscriptionHandle;
pub use channel::EventChannel;
pub use poller::EventPoller;
pub use handler::EventHandler;

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
}