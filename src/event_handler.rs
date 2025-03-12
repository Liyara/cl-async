/*use log::error;
use bitflags::bitflags;
use crate::event_poller::{EventPoller, EventPollerError, EventProcessAction, Key};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IOEventType: u32 {
        const READABLE = 1 << 0;
        const WRITABLE = 1 << 1;
    }
}

impl IOEventType {
    pub fn is_readable(&self) -> bool {
        self.contains(IOEventType::READABLE)
    }

    pub fn is_writable(&self) -> bool {
        self.contains(IOEventType::WRITABLE)
    }

}

pub trait EventHandler : Send {
    fn on_io_event(&mut self, key: Key, event_type: IOEventType, poller: &EventPoller) -> EventProcessAction;
    fn on_closed(&mut self, _: Key);
    fn on_half_closed(&mut self, key: Key) -> bool { 
        self.on_closed(key); 
        false
    }
    fn handle_error(&mut self, _: Key, err: EventPollerError) { error!("{}", err); }
}*/