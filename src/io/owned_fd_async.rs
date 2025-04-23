use std::{fmt, mem::ManuallyDrop, os::fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd}};

use super::IoOperation;


pub struct OwnedFdAsync(RawFd);

impl OwnedFdAsync {
    pub fn release(self) -> RawFd {
        let fd = self.0;
        std::mem::forget(self);
        fd
    }
}

impl AsRawFd for OwnedFdAsync {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl IntoRawFd for OwnedFdAsync {
    fn into_raw_fd(self) -> RawFd {
        ManuallyDrop::new(self).0
    }
}

impl FromRawFd for OwnedFdAsync {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        assert_ne!(fd, u32::MAX as RawFd);
        Self(fd)
    }
}

impl Drop for OwnedFdAsync {
    fn drop(&mut self) {
        if let Err(_) = crate::submit_io_operation(
            IoOperation::close_forget(self),
            None
        ) {
            if let Err(e) = syscall!(close(self.0)) {
                error!("Failed to close fd {}: {}", self.0, e);
            }
        }
    }
}

impl fmt::Debug for OwnedFdAsync {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "OwnedFdAsync({})", self.0)
    }
} 