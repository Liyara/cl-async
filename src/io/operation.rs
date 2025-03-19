use std::os::fd::{
    AsRawFd, 
    RawFd
};

use super::IOType;

pub struct IOOperation {
    fd: RawFd,
    buffer: Vec<u8>,
    len: usize,
    offset: Option<usize>,
    t: IOType,
}

impl IOOperation {

    pub fn length(&self) -> usize { self.len }
    pub fn buffer(&self) -> &[u8] { &self.buffer }
    pub fn buffer_mut(&mut self) -> &mut [u8] { &mut self.buffer }
    pub fn buffer_owned(self) -> Vec<u8> { self.buffer }
    pub fn io_type(&self) -> IOType { self.t }
    pub fn offset(&self) -> Option<usize> { self.offset }

    pub fn read(fd: RawFd, len: usize) -> Self {
        let buffer = Vec::with_capacity(len);
        Self {
            fd,
            buffer,
            offset: None,
            len,
            t: IOType::Read,
        }
    }

    pub fn read_at(fd: RawFd, offset: usize, len: usize) -> Self {
        let buffer = Vec::with_capacity(len);
        Self {
            fd,
            buffer,
            offset: Some(offset),
            len,
            t: IOType::Read,
        }
    }

    pub fn write(fd: RawFd, buffer: Vec<u8>) -> Self {
        let len = buffer.len();
        Self {
            fd,
            buffer,
            offset: None,
            len,
            t: IOType::Write,
        }
    }

    pub fn write_at(fd: RawFd, offset: usize, buffer: Vec<u8>) -> Self {
        let len = buffer.len();
        Self {
            fd,
            buffer,
            offset: Some(offset),
            len,
            t: IOType::Write,
        }
    }
}

impl AsRawFd for IOOperation {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}