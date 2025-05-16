use crate::io::{message::{IoSendMessage, PreparedIoMessage, PreparedIoMessageBuilder}, IoControlMessage, IoInputBuffer, IoSubmissionError};
use super::send::IoSendFlags;

pub struct IoSendMsgData {
    flags: IoSendFlags,
    prepared_msg: Option<PreparedIoMessage>,
}

impl IoSendMsgData {
    pub fn new(
        message: IoSendMessage,
        flags: IoSendFlags,
    ) -> Result<Self, IoSubmissionError> {

        let mut builder = PreparedIoMessageBuilder::new();

        if let Some(control) = message.control {
            let control_buffer = IoInputBuffer::new(
            IoControlMessage::try_prepare_multi(
                control
            ).map_err(|e| {
                IoSubmissionError::FailedToPrepareControlMessages(e)
            })?)?;

            builder.set_control(control_buffer);
        }

        if let Some(data) = message.data {
            builder.set_buffers(data)?;
        }

        if let Some(address) = message.address {
            builder.set_address(address)?;
        }
        
        Ok(Self {
            flags,
            prepared_msg: Some(builder.build()),
        })
    }
}

impl super::CompletableOperation for IoSendMsgData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletion {
        crate::io::IoCompletion::Write(crate::io::completion_data::IoWriteCompletion {
            bytes_written: result_code as usize,
        })
    }
}

impl super::AsUringEntry for IoSendMsgData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {

        let prepared_msg = self.prepared_msg.as_ref().unwrap();

        io_uring::opcode::SendMsg::new(
            io_uring::types::Fd(fd),
        prepared_msg.as_ptr()
        ).flags(self.flags.bits() as u32)
        .build().user_data(key.as_u64())
    }
}