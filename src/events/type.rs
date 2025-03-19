use std::fmt;
use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct EventType: i32 {
        const READABLE = libc::EPOLLIN;
        const WRITABLE = libc::EPOLLOUT;
        const ERROR = libc::EPOLLERR;
        const HUP = libc::EPOLLHUP;
        const RDHUP = libc::EPOLLRDHUP;
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut flags = vec![];
        if self.contains(EventType::READABLE) {
            flags.push("READABLE");
        }
        if self.contains(EventType::WRITABLE) {
            flags.push("WRITABLE");
        }
        if self.contains(EventType::ERROR) {
            flags.push("ERROR");
        }
        if self.contains(EventType::HUP) {
            flags.push("HUP");
        }
        if self.contains(EventType::RDHUP) {
            flags.push("RDHUP");
        }
        write!(f, "EventType {{ {:?} }}", flags.join(" | "))
    }
}