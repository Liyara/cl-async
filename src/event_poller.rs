
use std::{fmt, os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd}, sync::{atomic::{AtomicU64, Ordering}, Arc}};
use bitflags::bitflags;
use dashmap::DashMap;
use log::{error, info};
use thiserror::Error;

use crate::syscall;

#[derive(Debug, Error)]
pub enum OSError {

    #[error("Maximum number of file descriptors reached")]
    MaxFdReached,

    #[error("Not enough memory")]
    NotEnoughMemory,

    #[error("File could not be read")]
    FileCouldNotBeRead,

    #[error("Invalid file descriptor")]
    InvalidFd,

    #[error("File descriptor is already registered with this resource")]
    FdAlreadyRegistered,

    #[error("Invalid pointer")]
    InvalidPointer,

    #[error("file descriptor not supported for this operation")]
    FdNotSupported,

    #[error("The operartion was interrupted")]
    OperationInterrupted,

    #[error("The file descriptor has not been registered with this resource")]
    FdNotRegistered,

    #[error("Invalid operation")]
    InvalidOperation,

    #[error("Unknown OS error")]
    UnknownError,

    #[error("OS Error: {0}")]
    Generic(i32),
}

impl From<std::io::Error> for OSError {
     fn from(error: std::io::Error) -> Self {
        let os_error = match error.raw_os_error() {
            Some(code) => code,
            None => return OSError::UnknownError,
        };

        match os_error {
            libc::EINVAL => OSError::InvalidOperation,
            libc::EMFILE | libc::ENFILE => OSError::MaxFdReached,
            libc::ENOMEM => OSError::NotEnoughMemory,
            libc::EACCES => OSError::FileCouldNotBeRead,
            libc::EBADF => OSError::InvalidFd,
            libc::EEXIST => OSError::FdAlreadyRegistered,
            libc::EFAULT => OSError::InvalidPointer,
            libc::ENOENT => OSError::FdNotRegistered,
            libc::EPERM => OSError::FdNotSupported,
            libc::EINTR => OSError::OperationInterrupted,
            _ => OSError::Generic(os_error),
        }
     }
}

#[derive(Debug, Error)]
pub enum EventPollerError {
    #[error("Failed to create epoll file descriptor: {source}")]
    FailedToCreateEpollFd {
        source: OSError,
    },

    #[error("Failed to register interest: {source}")]
    FailedToRegisterInterest {
        source: OSError,
    },

    #[error("Failed to modify interest: {source}")]
    FailedToModifyInterest {
        source: OSError,
    },

    #[error("Failed to deregister interest: {source}")]
    FailedToDeregisterInterest {
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

const COMMON_FLAGS: i32 = libc::EPOLLET | libc::EPOLLHUP | libc::EPOLLRDHUP;
bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct InterestType: i32 {
        const READ = libc::EPOLLIN | COMMON_FLAGS;
        const WRITE = libc::EPOLLOUT | COMMON_FLAGS;
    }
}

impl InterestType {
    pub fn is_readable(&self) -> bool {
        self.contains(InterestType::READ)
    }

    pub fn is_writable(&self) -> bool {
        self.contains(InterestType::WRITE)
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct EventType: i32 {
        const READABLE = libc::EPOLLIN;
        const WRITABLE = libc::EPOLLOUT;
        const ERROR = libc::EPOLLERR;
        const HUP = libc::EPOLLHUP;
        const RDHUP = libc::EPOLLRDHUP;
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut flags = vec![];
        if self.contains(EventType::READABLE) {
            flags.push("READABLE");
        }
        if self.contains(EventType::WRITABLE) {
            flags.push("WRITABLE");
        }
        if self.contains(EventType::ERROR) {
            flags.push("ERROR");
        }
        if self.contains(EventType::HUP) {
            flags.push("HUP");
        }
        if self.contains(EventType::RDHUP) {
            flags.push("RDHUP");
        }
        write!(f, "EventType {{ {:?} }}", flags.join(" | "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollEvent {
    pub event_type: EventType,
    pub key: Key,
}

impl PollEvent {

    fn from_libc_epoll_event(event: &libc::epoll_event) -> Self {
        Self {
            event_type: EventType::from_bits_truncate(event.events as i32),
            key: Key::new(event.u64),
        }
    }

    pub fn is_error(&self) -> bool {
        self.event_type.contains(EventType::ERROR)
    }

    pub fn is_readable(&self) -> bool {
        self.event_type.contains(EventType::READABLE)
    }

    pub fn is_writable(&self) -> bool {
        self.event_type.contains(EventType::WRITABLE)
    }

    pub fn is_hup(&self) -> bool {
        self.event_type.contains(EventType::HUP)
    }

    pub fn is_rdhup(&self) -> bool {
        self.event_type.contains(EventType::RDHUP)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key(u64);

impl Key {
    pub fn new(val: u64) -> Self {
        Self(val)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

static MINIMUM_KEY_VALUE: u64 = 100;

#[derive(Debug, Clone)]
pub struct KeyGenerator {
    inner: Arc<AtomicU64>
}

impl KeyGenerator {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicU64::new(MINIMUM_KEY_VALUE)),
        }
    }

    pub fn get(&self) -> Key {
        Key::new(self.inner.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone)]
pub struct EventRegistry {
    inner: Arc<OwnedFd>,
    generator: KeyGenerator,
    key_fds: Arc<DashMap<Key, RawFd>>,
}

impl EventRegistry {

    pub fn register_interest<T>(
        &self,
        source: &T,
        interest_type: InterestType,
    ) -> Result<Key, EventPollerError> where T: AsRawFd {
        let key = self.generator.get();
        let fd = source.as_raw_fd();
        let mut event = libc::epoll_event {
            events: interest_type.bits() as u32,
            u64: key.as_u64(),
        };
        syscall!(
            epoll_ctl(self.inner.as_raw_fd(), libc::EPOLL_CTL_ADD, fd, &mut event)
        ).map_err(|e| 
            EventPollerError::FailedToRegisterInterest { source: e.into() }
        )?;
        self.key_fds.insert(key, fd);
        info!(
            "Registered interest for fd {} with key {} and interest type {} in {}",
            fd,
            key.as_u64(),
            interest_type.bits(),
            self.inner.as_raw_fd() 
        );
        Ok(key)
    }

    pub fn modify_interest(
        &self,
        key: Key,
        interest_type: InterestType,
    ) -> Result<(), EventPollerError> {
        let fd = self.key_fds.get(&key).ok_or(EventPollerError::FailedToModifyInterest {
            source: OSError::FdNotRegistered
        })?;
        let mut event = libc::epoll_event {
            events: interest_type.bits() as u32,
            u64: key.as_u64(),
        };
        syscall!(
            epoll_ctl(self.inner.as_raw_fd(), libc::EPOLL_CTL_MOD, *fd, &mut event)
        ).map_err(|e|
            EventPollerError::FailedToModifyInterest { source: e.into() }
        )?;
        info!(
            "Modified interest for fd {} with key {} and interest type {} in {}",
            *fd,
            key.as_u64(),
            interest_type.bits(),
            self.inner.as_raw_fd()
        );
        Ok(())
    }

    pub fn deregister_interest(&self, key: Key) -> Result<(), EventPollerError> {
        let (key, fd) = self.key_fds.remove(&key).ok_or(EventPollerError::FailedToDeregisterInterest {
            source: OSError::FdNotRegistered
        })?;
        syscall!(epoll_ctl(self.inner.as_raw_fd(), libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut())).map_err(|e|
            EventPollerError::FailedToDeregisterInterest { source: e.into() }
        )?;
        info!(
            "Deregistered interest for fd {} with key {} in {}",
            fd,
            key.as_u64(),
            self.inner.as_raw_fd()
        );
        Ok(())
    }

}

#[derive(Clone)]
pub struct EventPoller {
    inner: Arc<OwnedFd>,
    event_registry: EventRegistry,
    buffer: Vec<libc::epoll_event>,
    buffer_size: usize,
}

impl EventPoller {

    pub fn new(buffer_size: usize) -> Result<Self, EventPollerError> {
        let inner = Arc::new(unsafe {
            OwnedFd::from_raw_fd(Self::epoll_create()?)
        });
        let event_registry = EventRegistry {
            inner: Arc::clone(&inner),
            generator: KeyGenerator::new(),
            key_fds: Arc::new(DashMap::new()),
        };
        Ok(Self {
            inner,
            event_registry,
            buffer: Vec::with_capacity(buffer_size),
            buffer_size,
        })
    }

    pub fn registry(&self) -> EventRegistry {
        self.event_registry.clone()
    }

    pub fn poll_events(&mut self, events_out: &mut Vec<PollEvent>, timeout_ms: Option<i32>) -> Result<(), EventPollerError> {
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

        info!(
            "Polled {} events in {} with timeout {}",
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
        events_out.extend(self.buffer.iter().map(PollEvent::from_libc_epoll_event));
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