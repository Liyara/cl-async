use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path};

use crate::io::IoSubmissionError;

pub struct IoUnlinkData {
    path: CString,
}

impl IoUnlinkData {
    pub fn new(path: &Path) -> Result<Self, IoSubmissionError> {
        let path = CString::new(path.as_os_str().as_bytes())?;
        Ok(Self { path })
    }
}

impl super::CompletableOperation for IoUnlinkData {}

impl super::AsUringEntry for IoUnlinkData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::UnlinkAt::new(
            io_uring::types::Fd(fd),
            self.path.as_ptr(),
        ).build().user_data(key.as_u64())
    }
}