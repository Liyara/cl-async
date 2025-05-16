#[derive(Clone, Copy)]
#[repr(i32)]
pub enum IoShutdownType {
    Read = libc::SHUT_RD,
    Write = libc::SHUT_WR,
    Both = libc::SHUT_RDWR,
}

pub struct IoShutdownData {
    how: IoShutdownType,
}

impl IoShutdownData {
    pub fn new(how: IoShutdownType) -> Self {
        Self { how }
    }
}

impl super::CompletableOperation for IoShutdownData {}

impl super::AsUringEntry for IoShutdownData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::Shutdown::new(
            io_uring::types::Fd(fd),
            self.how as i32,
        ).build().user_data(key.as_u64())
    }
}