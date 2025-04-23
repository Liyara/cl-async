use std::os::fd::RawFd;

pub struct IoFDatasyncData;

impl super::CompletableOperation for IoFDatasyncData {
    fn get_completion(&mut self, _: u32) -> crate::io::IoCompletionResult {
        Ok(crate::io::IoCompletion::Success)
    }
}

impl super::AsUringEntry for IoFDatasyncData {
    fn as_uring_entry(&mut self, fd: RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::Fsync::new(
            io_uring::types::Fd(fd),
        ).build().user_data(key.as_u64())
    }
}