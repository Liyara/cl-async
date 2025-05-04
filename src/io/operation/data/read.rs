use crate::io::{IoBuffer, IoOutputBuffer};

pub struct IoReadData {
    buffer: Option<IoOutputBuffer>,
    offset: usize,
}

impl IoReadData {
    pub fn new(buffer: IoOutputBuffer, offset: usize) -> Self {
        Self {
            buffer: Some(buffer),
            offset,
        }
    }
}

impl super::CompletableOperation for IoReadData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletionResult {
        let buffer = self.buffer.take().ok_or(
            crate::io::IoOperationError::NoData
        )?;
        let len = result_code as usize;
        let data = buffer.into_bytes(len)?;
        Ok(crate::io::IoCompletion::Read(crate::io::completion_data::IoReadCompletion {
            data
        }))
    }
}

impl super::AsUringEntry for IoReadData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        
        let buffer = self.buffer.as_mut().unwrap();

        unsafe {
            io_uring::opcode::Read::new(
                io_uring::types::Fd(fd),
                buffer.as_mut_ptr(),
                buffer.writable_len() as u32,
            ).offset(self.offset as u64)
            .build().user_data(key.as_u64())
        }
    }
}