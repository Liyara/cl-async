use std::os::fd::RawFd;

use io_uring::types::FsyncFlags;

pub struct IoFDatasyncData;

impl super::CompletableOperation for IoFDatasyncData {}

impl super::AsUringEntry for IoFDatasyncData {
    fn as_uring_entry(&mut self, fd: RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::Fsync::new(
            io_uring::types::Fd(fd),
        ).flags(FsyncFlags::DATASYNC).build().user_data(key.as_u64())
    }
}