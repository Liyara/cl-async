use std::os::fd::RawFd;
use bitflags::bitflags;
use parking_lot::Mutex;

use crate::{io::{IoDoubleOutputBuffer, IoError, IoOperationError, PreparedIoMessage}, Key};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IoRecvMsgInputFlags: i32 {
        const TRUNCATE = libc::MSG_TRUNC;
        const OUT_OF_BAND = libc::MSG_OOB;
        const ERR_QUEUE = libc::MSG_ERRQUEUE;
        const PEEK = libc::MSG_PEEK;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IoRecvMsgOutputFlags: i32 {
        const END_OF_RECORD = libc::MSG_EOR;
        const TRUNCATED = libc::MSG_TRUNC;
        const CONTROL_TRUNCATED = libc::MSG_CTRUNC;
        const OUT_OF_BAND = libc::MSG_OOB;
        const ERR_QUEUE = libc::MSG_ERRQUEUE;
    }
}
pub struct IoRecvMsgData {
    pub flags: IoRecvMsgInputFlags,
    prepared_msg: Mutex<Option<PreparedIoMessage<IoDoubleOutputBuffer>>>,
}

impl IoRecvMsgData {
    pub fn new(
        buffers: Option<IoDoubleOutputBuffer>,
        control: Option<Vec<u8>>,
        flags: IoRecvMsgInputFlags,
    ) -> Result<Self, IoError> {

        Ok(
            Self {
                flags,
                prepared_msg: Mutex::new(Some(PreparedIoMessage::new(
                    buffers,
                    control,
                    PreparedIoMessageAddressDirection::Output,
                ).map_err(|e| {
                    IoOperationError::from(e)
                })?))
            }
        )
    }
}

impl super::CompletableOperation for IoRecvMsgData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletionResult {

        if let Some(prepared_msg) = self.prepared_msg.lock().take() {
            Ok(crate::io::IoCompletion::Msg(crate::io::completion::data::IoMsgCompletion {
                msg: prepared_msg.into_message(result_code as usize)?,
            }))
        } else {
            Err(crate::io::IoOperationError::NoData)
        }
    }
}

impl super::AsUringEntry for IoRecvMsgData {
    
    fn as_uring_entry(&mut self, fd: RawFd, key: Key) -> io_uring::squeue::Entry {

        let mut binding = self.prepared_msg.lock();
        let prepared_msg = binding.as_mut().unwrap();
        let msghdr = prepared_msg.as_mut();

        io_uring::opcode::RecvMsg::new(
            io_uring::types::Fd(fd),
            msghdr as *mut _
        ).flags(self.flags.bits() as u32)
        .build().user_data(key.as_u64())
    }
}