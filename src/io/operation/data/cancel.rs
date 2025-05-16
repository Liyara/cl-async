use std::os::fd::RawFd;

use crate::Key;

pub struct IoCancelData {
    key: Key,
}

impl IoCancelData {
    pub fn new(key: Key) -> Self {
        Self { key }
    }
}

impl super::CompletableOperation for IoCancelData {
    fn get_completion(&mut self, _: u32) -> crate::io::IoCompletion {
        crate::io::IoCompletion::Success
    }
}

impl super::AsUringEntry for IoCancelData {
    fn as_uring_entry(&mut self, _: RawFd, key: Key) -> io_uring::squeue::Entry {
        io_uring::opcode::AsyncCancel::new(
            self.key.as_u64()
        ).build().user_data(key.as_u64())
    }
}