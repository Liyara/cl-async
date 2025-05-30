use bytes::BytesMut;

use crate::io::{failure::{data::IoReadFailure, IoFailure}, IoBytesMutRecovery, IoOutputBuffer};

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

    pub fn buffer(&self) -> Option<&IoOutputBuffer> {
        self.buffer.as_ref()
    }

    pub fn into_buffer(mut self) -> Option<IoOutputBuffer> {
        self.buffer.take()
    }
}

impl super::CompletableOperation for IoReadData {

    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletion {

        let bytes_read = result_code as usize;

        let buffer = self.buffer.take().map(|b| {
            match b.into_bytes(bytes_read) {
                Ok(buffer) => buffer,
                Err(e) => {
                    warn!("cl-async: read(): Issue while advancing buffer: {e}");
                    e.into_buffer()
                }
            }
        }).unwrap_or_else(|| {
            warn!("cl-async: read(): Expected buffer but got None; returning empty buffer");
            BytesMut::new()
        });

        crate::io::IoCompletion::Read(crate::io::completion_data::IoReadCompletion {
            buffer,
            bytes_read,
        })
    }

    fn get_failure(&mut self) -> crate::io::failure::IoFailure {
        IoFailure::Read(IoReadFailure {
            buffer: self.buffer.take().map(|b| {
                b.into_bytes_unchecked()
            }).unwrap_or_else(|| {
                warn!("cl-async: read(): Expected buffer but got None; returning empty buffer");
                BytesMut::new()
            }),
        })
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

impl IoBytesMutRecovery for IoReadData {

    fn as_bytes_mut(&self) -> Option<&BytesMut> {
        self.buffer.as_ref().map(|b| {
            b.as_bytes()
        })
    }

    fn into_bytes_mut(self) -> Option<BytesMut> {
        self.buffer.map(|b| {
            b.into_bytes_unchecked()
        })
    }
    
    fn take_bytes_mut(&mut self) -> Option<BytesMut> {
        self.buffer.take().map(|b| {
            b.into_bytes_unchecked()
        })
    }
}