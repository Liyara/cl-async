use std::os::fd::RawFd;

pub struct IoCloseData;

impl super::CompletableOperation for IoCloseData {
    fn get_completion(&mut self, _: u32) -> crate::io::IoCompletionResult {
        Ok(crate::io::IoCompletion::Success)
    }
}

impl super::AsUringEntry for IoCloseData {
    fn as_uring_entry(&mut self, fd: RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::Close::new(
            io_uring::types::Fd(fd)
        ).build().user_data(key.as_u64())
    }
}