use std::os::fd::RawFd;

use bitflags::bitflags;

use super::CompletableOperation;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IoSpliceFlags: u32 {
        const NONE = 0;
        const MOVE = libc::SPLICE_F_MOVE;
        const GIFT = libc::SPLICE_F_GIFT;
        const MORE = libc::SPLICE_F_MORE;
        // NONBLOCK not included as IO Uring makes it redundant
    }
}

pub struct IoSpliceData {
    pub fd_out: RawFd,
    pub in_offset: usize,
    pub out_offset: usize,
    pub bytes: usize,
    pub flags: IoSpliceFlags,
}

impl CompletableOperation for IoSpliceData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletion {
        crate::io::IoCompletion::Write(crate::io::completion_data::IoWriteCompletion {
            bytes_written: result_code as usize,
        })
    }
}

impl super::AsUringEntry for IoSpliceData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::Splice::new(
            io_uring::types::Fd(fd),
            self.in_offset as _,
            io_uring::types::Fd(self.fd_out),
            self.out_offset as _,
            self.bytes as _,
        ).flags(self.flags.bits())
        .build().user_data(key.as_u64())
    }
}