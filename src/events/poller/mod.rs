pub mod registry;
pub mod interest_type;

use std::os::fd::{
    AsRawFd, 
    FromRawFd, 
    OwnedFd, 
    RawFd
};

use std::sync::Arc;
use dashmap::DashMap;
use thiserror::Error;
use crate::key::KeyGenerator;

use crate::{
    syscall, 
    OSError
};

pub use self::interest_type::InterestType;
pub use self::registry::EventPollerRegistry;

use super::Event;

#[derive(Debug, Error)]
pub enum EventPollerError {
    #[error("Failed to create epoll file descriptor: {source}")]
    FailedToCreateEpollFd {
        source: OSError,
    },

    #[error("Failed to poll events: {source}")]
    FailedToPollEvents {
        source: OSError,
    },

    #[error("Failed to set epoll flags: {source}")]
    FailedToSetEpollFlags {
        source: OSError,
    }
}

#[derive(Clone)]
pub struct EventPoller {
    inner: Arc<OwnedFd>,
    event_registry: EventPollerRegistry,
    buffer: Vec<libc::epoll_event>,
    buffer_size: usize,
}

impl EventPoller {

    pub fn new(buffer_size: usize) -> Result<Self, EventPollerError> {
        let inner = Arc::new(unsafe {
            OwnedFd::from_raw_fd(Self::epoll_create()?)
        });
        let event_registry = EventPollerRegistry {
            inner: Arc::clone(&inner),
            generator: KeyGenerator::new(100),
            key_fds: Arc::new(DashMap::new()),
        };
        Ok(Self {
            inner,
            event_registry,
            buffer: Vec::with_capacity(buffer_size),
            buffer_size,
        })
    }

    pub fn registry(&self) -> EventPollerRegistry {
        self.event_registry.clone()
    }

    pub fn poll_events(&mut self, events_out: &mut Vec<Event>, timeout_ms: Option<i32>) -> Result<(), EventPollerError> {
        let timeout: i32 = timeout_ms.unwrap_or(-1);

        events_out.clear();
        self.buffer.clear();

        let n = syscall!(
            epoll_wait(
                self.inner.as_raw_fd(), 
                self.buffer.as_mut_ptr(),
                self.buffer_size as i32, 
                timeout
            )
        ).map_err(|e| EventPollerError::FailedToPollEvents { source: e.into() })?;

        log::info!(
            "Polled {} events in fd {} with timeout {}",
            n,
            self.inner.as_raw_fd(),
            timeout
        );

        /* 
            Safe because the OS grauntees that 0..self.events_len are filled with valid data
            and the number of events is always less than or equal to the capacity
        */
        unsafe { self.buffer.set_len(n as usize); }
        
        events_out.clear();
        events_out.extend(self.buffer.iter().map(Event::from_libc_epoll_event));
        Ok(())
    }

    fn epoll_create() -> Result<RawFd, EventPollerError> {
        let epoll_fd = syscall!(
            epoll_create1(0)
        ).map_err(|e| EventPollerError::FailedToCreateEpollFd { source: e.into() })?;
        if let Ok(flags) = syscall!(fcntl(epoll_fd, libc::F_GETFD)) {
            syscall!(
                fcntl(epoll_fd, libc::F_SETFD, flags | libc::FD_CLOEXEC)
            ).map_err(|e| EventPollerError::FailedToSetEpollFlags { source: e.into() })?;
        }
        Ok(epoll_fd)
    }
}