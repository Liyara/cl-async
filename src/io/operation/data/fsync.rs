use std::os::fd::RawFd;

pub struct IoFSyncData;

impl super::CompletableOperation for IoFSyncData {}

impl super::AsUringEntry for IoFSyncData {
    fn as_uring_entry(&mut self, fd: RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::Fsync::new(
            io_uring::types::Fd(fd),
        ).build().user_data(key.as_u64())
    }
}