use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path};

use crate::io::IoSubmissionError;

pub struct IoRenameData {
    old_path: CString,
    new_path: CString,
}

impl IoRenameData {
    pub fn new(old_path: &Path, new_path: &Path) -> Result<Self, IoSubmissionError> {
        let old_path = CString::new(old_path.as_os_str().as_bytes())?;
        let new_path = CString::new(new_path.as_os_str().as_bytes())?;
        Ok(Self { old_path, new_path })
    }
}

impl super::CompletableOperation for IoRenameData {}

impl super::AsUringEntry for IoRenameData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::RenameAt::new(
            io_uring::types::Fd(fd),
            self.old_path.as_ptr(),
            io_uring::types::Fd(fd),
            self.new_path.as_ptr(),
        ).build().user_data(key.as_u64())
    }
}