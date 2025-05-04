use bitflags::bitflags;
use crate::io::{IoBuffer, IoOutputBuffer};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IoRecvFlags: i32 {
        const ERR_QUEUE = libc::MSG_ERRQUEUE;
        const OUT_OF_BAND = libc::MSG_OOB;
        const PEEK = libc::MSG_PEEK;
        const TRUNCATE = libc::MSG_TRUNC;
    }
}

pub struct IoRecvData {
    buffer: Option<IoOutputBuffer>,
    flags: IoRecvFlags,
}

impl IoRecvData {
    pub fn new(buffer: IoOutputBuffer, flags: IoRecvFlags) -> Self {
        Self {
            buffer: Some(buffer),
            flags,
        }
    }

    pub fn set_flags(&mut self, flags: IoRecvFlags) {
        self.flags = flags;
    }
}

impl super::CompletableOperation for IoRecvData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletionResult {
        let buffer = self.buffer.take().ok_or(
            crate::io::IoOperationError::NoData
        )?;
        let len = result_code as usize;
        let data = buffer.into_bytes(len)?;
        Ok(crate::io::IoCompletion::Read(crate::io::completion_data::IoReadCompletion {
            data,
        }))
    }
}

impl super::AsUringEntry for IoRecvData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        let buffer = self.buffer.as_mut().unwrap();

        unsafe {
            io_uring::opcode::Recv::new(
                io_uring::types::Fd(fd),
                buffer.as_mut_ptr(),
                buffer.writable_len() as u32,
            ).flags(self.flags.bits())
            .build().user_data(key.as_u64())
        }
    }
}