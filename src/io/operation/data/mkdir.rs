use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path};

use crate::io::IoSubmissionError;

use super::open_at::IoFileSystemMode;

pub struct IoMkdirData {
    path: CString,
    mode: IoFileSystemMode,
}

impl IoMkdirData {
    pub fn new(path: &Path, mode: IoFileSystemMode) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            path: CString::new(path.as_os_str().as_bytes())?,
            mode
        })
    }
}

impl super::CompletableOperation for IoMkdirData {}

impl super::AsUringEntry for IoMkdirData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::MkDirAt::new(
            io_uring::types::Fd(fd),
            self.path.as_ptr(),
        ).mode(u32::from(self.mode))
        .build().user_data(key.as_u64())
    }
}