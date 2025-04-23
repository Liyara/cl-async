use std::{os::fd::{AsRawFd, OwnedFd, RawFd}, sync::Arc};

use dashmap::DashMap;
use thiserror::Error;

use crate::{
    events::EventSource, 
    key::KeyGenerator, 
    Key, 
    OsError
};

use super::InterestType;

#[derive(Debug, Error)]

pub enum EventPollerRegistryError {
    #[error("Failed to register interest: {source}")]
    FailedToRegisterInterest {
        source: OsError,
    },

    #[error("Failed to modify interest: {source}")]
    FailedToModifyInterest {
        source: OsError,
    },

    #[error("Failed to deregister interest: {source}")]
    FailedToDeregisterInterest {
        source: OsError,
    },
}


#[derive(Debug, Clone)]
pub struct EventPollerRegistry {
    pub (in crate::events::poller) inner: Arc<OwnedFd>,
    pub (in crate::events::poller) generator: KeyGenerator,
    pub (in crate::events::poller) key_fds: Arc<DashMap<Key, RawFd>>,
}

impl EventPollerRegistry {

    pub fn register_interest(
        &self,
        source: EventSource,
        interest_type: InterestType,
    ) -> Result<Key, EventPollerRegistryError> {
        let key = self.generator.get();
        let fd = source.as_raw_fd();
        let mut event = libc::epoll_event {
            events: interest_type.bits() as u32,
            u64: key.as_u64(),
        };
        syscall!(
            epoll_ctl(self.inner.as_raw_fd(), libc::EPOLL_CTL_ADD, fd, &mut event)
        ).map_err(|e| 
            EventPollerRegistryError::FailedToRegisterInterest { source: e.into() }
        )?;
        self.key_fds.insert(key, fd);
        
        info!(
            "cl-async: Registered interest for fd {} with key {} and interest type {} in {}",
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
    ) -> Result<(), EventPollerRegistryError> {
        let fd = self.key_fds.get(&key).ok_or(
            EventPollerRegistryError::FailedToModifyInterest {
                source: OsError::NotFound
            }
        )?;
        let mut event = libc::epoll_event {
            events: interest_type.bits() as u32,
            u64: key.as_u64(),
        };
        syscall!(
            epoll_ctl(self.inner.as_raw_fd(), libc::EPOLL_CTL_MOD, *fd, &mut event)
        ).map_err(|e|
            EventPollerRegistryError::FailedToModifyInterest { source: e.into() }
        )?;

        info!(
            "cl-async: Modified interest for fd {} with key {} and interest type {} in {}",
            *fd,
            key.as_u64(),
            interest_type.bits(),
            self.inner.as_raw_fd()
        );

        Ok(())
    }

    pub fn deregister_interest(&self, key: Key) -> Result<(), EventPollerRegistryError> {
        let (key, fd) = self.key_fds.remove(&key).ok_or(
            EventPollerRegistryError::FailedToDeregisterInterest {
                source: OsError::NotFound
            }
        )?;
        syscall!(epoll_ctl(self.inner.as_raw_fd(), libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut())).map_err(|e|
            EventPollerRegistryError::FailedToDeregisterInterest { source: e.into() }
        )?;

        info!(
            "cl-async: Deregistered interest for fd {} with key {} in {}",
            fd,
            key.as_u64(),
            self.inner.as_raw_fd()
        );

        Ok(())
    }

}