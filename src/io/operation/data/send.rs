use bitflags::bitflags;

use crate::io::IoInputBuffer;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IoSendFlags: i32 {
        const CONFIRM = libc::MSG_CONFIRM;
        const DONT_ROUTE = libc::MSG_DONTROUTE;
        const END_OF_RECORD = libc::MSG_EOR;
        const MORE = libc::MSG_MORE;
        const NO_SIGNAL = libc::MSG_NOSIGNAL;
        const OUT_OF_BAND = libc::MSG_OOB;
        const ZERO_COPY = libc::MSG_ZEROCOPY;
    }
}

pub struct IoSendData {
    buffer: IoInputBuffer,
    flags: IoSendFlags,
}

impl IoSendData {
    pub fn new(write_data: IoInputBuffer, flags: IoSendFlags) -> Self {
        Self {
            buffer: write_data,
            flags,
        }
    }

    pub fn buffer(&self) -> &IoInputBuffer {
        &self.buffer
    }

    pub fn into_buffer(self) -> IoInputBuffer {
        self.buffer
    }
}

impl super::CompletableOperation for IoSendData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletion {
        crate::io::IoCompletion::Write(crate::io::completion_data::IoWriteCompletion {
            bytes_written: result_code as usize
        })
    }
}

impl super::AsUringEntry for IoSendData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        let buffer = &mut self.buffer;

        unsafe {
            io_uring::opcode::Send::new(
                io_uring::types::Fd(fd),
                buffer.as_ptr(),
                buffer.readable_len() as u32,
            ).flags(self.flags.bits())
            .build().user_data(key.as_u64())
        }
    }
}