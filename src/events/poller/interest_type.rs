use bitflags::bitflags;

const COMMON_FLAGS: i32 = libc::EPOLLHUP | libc::EPOLLRDHUP | libc::EPOLLET;
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct InterestType: i32 {
        const READ = libc::EPOLLIN | COMMON_FLAGS;
        const WRITE = libc::EPOLLOUT | COMMON_FLAGS;
        const ONESHOT = libc::EPOLLONESHOT;
    }
}

impl InterestType {
    pub fn is_readable(&self) -> bool {
        self.contains(InterestType::READ)
    }

    pub fn is_writable(&self) -> bool {
        self.contains(InterestType::WRITE)
    }

    pub fn is_oneshot(&self) -> bool {
        self.contains(InterestType::ONESHOT)
    }
}