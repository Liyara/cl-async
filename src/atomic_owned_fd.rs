use std::{mem::ManuallyDrop, os::fd::{AsRawFd, FromRawFd, RawFd}, sync::atomic::AtomicI32};

const INVALID_FD: RawFd = u32::MAX as RawFd;

#[derive(Debug)]
pub struct AtomicOwnedFd {
    inner: AtomicI32
}

impl AtomicOwnedFd {

    /*
        SAFETY:

            This function is unsafe because it creates an AtomicOwnedFd
            from a raw file descriptor. The caller must ensure that the
            file descriptor is valid
    */
    pub unsafe fn new(fd: RawFd) -> Self {
        assert_ne!(fd, INVALID_FD);
        Self {
            inner: AtomicI32::new(fd as i32)
        }
    }

    pub fn load(&self) -> RawFd {
        self.inner.load(std::sync::atomic::Ordering::Relaxed) as RawFd
    }

    /*
        SAFETY:

            This function is unsafe because it returns an open FD.
            The caller must ensure that the returned FD is cleaned up.

            The caller must also ensure the provided FD is valid
    */
    pub unsafe fn swap(&self, new_fd: RawFd) -> RawFd {
        assert_ne!(new_fd, INVALID_FD);
        self.inner.swap(new_fd, std::sync::atomic::Ordering::SeqCst) as RawFd
    }

    /*
        SAFETY:

            This function is unsafe because it returns an open FD.
            The caller must ensure that the returned FD is cleaned up.

    */
    pub unsafe fn release(&self) -> RawFd {
        self.inner.swap(INVALID_FD, std::sync::atomic::Ordering::SeqCst) as RawFd
    }

    pub fn is_valid(&self) -> bool { self.load() != INVALID_FD }
        
}

impl AsRawFd for AtomicOwnedFd {
    fn as_raw_fd(&self) -> RawFd {
        self.load()
    }
}

impl Into<RawFd> for AtomicOwnedFd {
    fn into(self) -> RawFd {
        ManuallyDrop::new(self).load()
    }
}

impl FromRawFd for AtomicOwnedFd {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        unsafe { Self::new(fd) }
    }
}

impl Drop for AtomicOwnedFd {
    fn drop(&mut self) {
        let fd = unsafe { self.release() };
        if fd != INVALID_FD {
            if let Err(e) = syscall!(close(fd)) {
                error!("cl-async: Failed to close fd {}: {}", fd, e);
            }
        }
    }
}