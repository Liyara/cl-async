use bitflags::bitflags;

const COMMON_FLAGS: i32 = libc::EPOLLHUP | libc::EPOLLRDHUP;
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct InterestType: i32 {
        const READ = libc::EPOLLIN | COMMON_FLAGS;
        const WRITE = libc::EPOLLOUT | COMMON_FLAGS;
        const EDGE_TRIGGERED = libc::EPOLLET;
    }
}

impl InterestType {
    pub fn is_readable(&self) -> bool {
        self.contains(InterestType::READ)
    }

    pub fn is_writable(&self) -> bool {
        self.contains(InterestType::WRITE)
    }

    pub fn is_edge_triggered(&self) -> bool {
        self.contains(InterestType::EDGE_TRIGGERED)
    }
}