use parking_lot::Mutex;
use crate::io::{IoDoubleInputBuffer, IoMessage, IoResult, PreparedIoMessage};
use super::send::IoSendFlags;

pub struct IoSendMsgData {
    flags: IoSendFlags,
    prepared_msg: Mutex<Option<PreparedIoMessage<IoDoubleInputBuffer>>>,
}

impl IoSendMsgData {
    pub fn new(
        message: IoMessage,
        flags: IoSendFlags,
    ) -> IoResult<Self> {
        
        Ok(Self {
            flags,
            prepared_msg: Mutex::new(Some(message.prepare()?))
        })
    }
}

impl super::CompletableOperation for IoSendMsgData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletionResult {
        Ok(crate::io::IoCompletion::Write(crate::io::completion_data::IoWriteCompletion {
            bytes_written: result_code as usize,
        }))
    }
}

impl super::AsUringEntry for IoSendMsgData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        
        let mut binding = self.prepared_msg.lock();
        let prepared_msg = binding.as_mut().unwrap();
        let msghdr = prepared_msg.as_mut();

        io_uring::opcode::SendMsg::new(
            io_uring::types::Fd(fd),
            msghdr as *mut _,
        ).flags(self.flags.bits() as u32)
        .build().user_data(key.as_u64())
    }
}