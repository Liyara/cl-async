use std::os::fd::RawFd;
use crate::{io::IoCompletion, Key};

pub struct IoAcceptMultiData;

impl super::CompletableOperation for IoAcceptMultiData {
    fn get_completion(&mut self, result_code: u32) -> IoCompletion {
        crate::io::IoCompletion::Accept(
            crate::io::completion_data::IoAcceptCompletion {
                fd: result_code as RawFd,
                address: None
            }
        )
    }
}

impl super::AsUringEntry for IoAcceptMultiData {
    fn as_uring_entry(&mut self, fd: RawFd, key: Key) -> io_uring::squeue::Entry {
        io_uring::opcode::AcceptMulti::new(
            io_uring::types::Fd(fd)
        ).build().user_data(key.as_u64())
    }
}
