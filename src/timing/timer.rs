use std::{os::fd::{AsRawFd, FromRawFd, OwnedFd}, sync::Arc};

use crate::syscall;


pub struct Timer {
    fd: Arc<OwnedFd>,
}

impl Timer {
    pub fn new() -> Result<Self, std::io::Error> {
        let fd = syscall!(timerfd_create(libc::CLOCK_MONOTONIC, libc::TFD_NONBLOCK))?;
        Ok(Self {
            fd: Arc::new(unsafe { OwnedFd::from_raw_fd(fd) }),
        })
    }

    pub fn wait_for(&self) -> Result<(), std::io::Error> {
        syscall!(timerfd_settime(
            self.fd.as_raw_fd(),
            0,
            &libc::itimerspec {
                it_interval: libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                },
                it_value: libc::timespec {
                    tv_sec: 2,
                    tv_nsec: 0,
                },
            },
            std::ptr::null_mut()
        ))?;
        Ok(())
    }
}