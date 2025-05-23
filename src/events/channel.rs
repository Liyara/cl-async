use std::{
    os::fd::{
        AsRawFd, 
        FromRawFd, 
        RawFd
    }, 
    sync::Arc
};

use thiserror::Error;
use bitflags::bitflags;

use crate::AtomicOwnedFd;

#[derive(Error, Debug)]
pub enum EventChannelError {
    #[error("Failed to create event channel: {0}")]
    FailedToCreateEventChannel(String),

    #[error("Failed to send event: {0}")]
    FailedToSendEvent(String),

    #[error("Failed to receive event: {0}")]
    FailedToReceiveEvent(String),

    #[error("Partial read/write (expected 8 bytes, got {0})")]
    PartialTransfer(isize),
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct EventChannelFlags: i32 {
        const NONE = 0;
        const CLOEXEC = libc::EFD_CLOEXEC;
        const NONBLOCK = libc::EFD_NONBLOCK;
        const SEMAPHORE = libc::EFD_SEMAPHORE;
    }
}

impl Default for EventChannelFlags {
    fn default() -> Self {
        Self::NONE
    }
}

#[derive(Debug)]
pub struct EventChannel {
    fd: Arc<AtomicOwnedFd>,
}

impl EventChannel {

    pub fn new() -> Result<Self, EventChannelError> {
        Self::with_flags(
            EventChannelFlags::NONBLOCK |
            EventChannelFlags::CLOEXEC
        ) 
    }

    pub fn with_flags(flags: EventChannelFlags) -> Result<Self, EventChannelError> {
        let fd = syscall!(eventfd(0, flags.bits())).map_err(|e|
            EventChannelError::FailedToCreateEventChannel(e.to_string())
        )?;
        Ok(Self {
            fd: Arc::new(
                unsafe { AtomicOwnedFd::from_raw_fd(fd) }
            ),
        })
    }

    pub fn as_sender(&self) -> EventChannelSender {
        EventChannelSender::new(Arc::clone(&self.fd))
    }

    pub fn as_receiver(&self) -> EventChannelReceiver {
        EventChannelReceiver::new(Arc::clone(&self.fd))
    }

}

impl AsRawFd for EventChannel {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl AsRef<AtomicOwnedFd> for EventChannel {
    fn as_ref(&self) -> &AtomicOwnedFd {
        &self.fd
    }
}

#[derive(Debug, Clone)]
pub struct EventChannelSender {
    fd: Arc<AtomicOwnedFd>,
}

impl EventChannelSender {
    fn new(fd: Arc<AtomicOwnedFd>) -> Self {
        Self { fd }
    }

    pub fn send<T>(&self, value: T) -> Result<(), EventChannelError>
    where 
        T: Into<u64> 
    {
        let data = value.into().to_ne_bytes();
        let bytes_written = syscall!(
            write(self.fd.as_raw_fd(), data.as_ptr() as *const libc::c_void, std::mem::size_of::<u64>())
        ).map_err(|e| EventChannelError::FailedToSendEvent(e.to_string()))?;

        if bytes_written != 8 {
            return Err(EventChannelError::PartialTransfer(bytes_written));
        }

        Ok(())
    }

    pub fn signal(&self) -> Result<(), EventChannelError> {
        self.send(1u64)
    }
}

impl AsRawFd for EventChannelSender {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl AsRef<AtomicOwnedFd> for EventChannelSender {
    fn as_ref(&self) -> &AtomicOwnedFd {
        &self.fd
    }
}

#[derive(Debug, Clone)]
pub struct EventChannelReceiver {
    fd: Arc<AtomicOwnedFd>,
}

impl EventChannelReceiver {
    fn new(fd: Arc<AtomicOwnedFd>) -> Self {
        Self { fd }
    }

    pub fn _recv<T>(&self) -> Result<T, EventChannelError> 
    where
        T: From<u64>,
    {
        let mut data = [0u8; 8];
        let bytes_read = syscall!(
            read(self.fd.as_raw_fd(), data.as_mut_ptr() as *mut libc::c_void, std::mem::size_of::<u64>())
        ).map_err(|e| EventChannelError::FailedToReceiveEvent(e.to_string()))?;

        if bytes_read != 8 {
            return Err(EventChannelError::PartialTransfer(bytes_read));
        }

        Ok(T::from(u64::from_ne_bytes(data)))
    }

    pub fn drain(&self) -> Result<(), EventChannelError> {
        loop {
            let mut data = [0u8; 8];
            let result = syscall!(
                read(self.fd.as_raw_fd(), data.as_mut_ptr() as *mut libc::c_void, std::mem::size_of::<u64>())
            );

            if let Err(e) = result {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    break;
                } else {
                    return Err(EventChannelError::FailedToReceiveEvent(e.to_string()));
                }
            }

            let result = result.unwrap();
            if result != 8 {
                return Err(EventChannelError::PartialTransfer(result));
            }
        }
        Ok(())
    }

}

impl AsRawFd for EventChannelReceiver {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl AsRef<AtomicOwnedFd> for EventChannelReceiver {
    fn as_ref(&self) -> &AtomicOwnedFd {
        &self.fd
    }
}