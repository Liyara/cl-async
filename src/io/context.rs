use std::os::fd::{
    AsRawFd, 
    RawFd
};
use rustc_hash::FxHashMap;
use thiserror::Error;
use crate::{
    events::EventChannel, 
    Key
};

use super::{
    IOCompletion, 
    IOCompletionQueue, 
    IOEntry, 
    IOReadCompletion, 
    IOType, 
    IOWriteCompletion
};

#[derive(Debug, Error)]
pub enum IOContextError {

    #[error("Failed to create IO Uring: {source}")]
    FailedToCreateIoUring {
        source: std::io::Error,
    },

    #[error("Failed to submit entries to OS: {source}")]
    FailedToSubmitEntries {
        source: std::io::Error,
    },

    #[error("Failed to register event channel: {source}")]
    FailedToRegisterEventChannel {
        source: std::io::Error,
    },
    
    #[error("Failed to create prepare submission: {0}")]
    FailedToPrepareSubmission(#[from] io_uring::squeue::PushError),

    #[error("IO Uring error code received for key {0}: {1}")]
    IoUringError(u64, i32),
}

pub struct IOContext {
    inner: io_uring::IoUring,
    completion_queue: IOCompletionQueue,
    flight_map: FxHashMap<Key, IOEntry>,
}

impl IOContext {

    pub fn new(entries: u32) -> Result<Self, IOContextError> {
        Ok(Self {
            inner: io_uring::IoUring::new(entries).map_err(
                |e| IOContextError::FailedToCreateIoUring { source: e }
            )?,
            completion_queue: IOCompletionQueue::new(),
            flight_map: FxHashMap::default(),
        })
    }

    pub fn prepare_submission(&mut self, mut entry: IOEntry) -> Result<Key, IOContextError> {
        let key = entry.key;
        let offset = entry.op.offset();
        let ring_entry = match entry.op.io_type() {
            IOType::Read => {
                let mut read = io_uring::opcode::Read::new(
                    io_uring::types::Fd(entry.op.as_raw_fd()),
                    entry.op.buffer_mut().as_mut_ptr(),
                    entry.op.length() as u32,
                );
                
                if let Some(offset) = offset {
                    read = read.offset(offset as u64);
                }

                read.build().user_data(entry.key.as_u64())
            },
            IOType::Write => {
                let mut write = io_uring::opcode::Write::new(
                    io_uring::types::Fd(entry.op.as_raw_fd()),
                    entry.op.buffer().as_ptr(),
                    entry.op.length() as u32,
                );

                if let Some(offset) = offset {
                    write = write.offset(offset as u64);
                }
                
                write.build().user_data(entry.key.as_u64())
            },
        };

        unsafe { self.inner.submission().push(&ring_entry)?; }

        self.flight_map.insert(entry.key, entry);
        Ok(key)
    }

    pub fn submit(&self) -> Result<usize, IOContextError> {
        Ok(self.inner.submit().map_err(
            |e| IOContextError::FailedToSubmitEntries { source: e }
        )?)
    }

    pub fn blocking_submit(&self) -> Result<usize, IOContextError> {
        Ok(self.inner.submit_and_wait(self.flight_map.len()).map_err(
            |e| IOContextError::FailedToSubmitEntries { source: e }
        )?)
    }

    pub fn complete(&mut self) {
        while let Some(item) = self.inner.completion().next() {
            let result = item.result();
            let key = Key::new(item.user_data());

            let mut entry = match self.flight_map.remove(&key) {
                Some(entry) => entry,
                None => continue
            };

            let waker = entry.waker.take();

            if result < 0 {
                self.completion_queue.push(
                    key, 
                    Err(result)
                );
            } else {

                let completion = match entry.op.io_type() {
                    IOType::Read => {
                        let bytes_read = result as usize;
                        let mut buffer = entry.op.buffer_owned();
                        unsafe { buffer.set_len(bytes_read); }
                        IOCompletion::Read(IOReadCompletion { buffer })
                    },
                    IOType::Write => {
                        let bytes_written = result as usize;
                        IOCompletion::Write(IOWriteCompletion {
                            bytes_written,
                        })
                    },
                };

                self.completion_queue.push(key, Ok(completion));
            }

            if let Some(waker) = waker { waker.wake();}
        }
    }

    pub fn completion(&self) -> &IOCompletionQueue {
        &self.completion_queue
    }

    pub fn register_event_channel(
        &mut self, 
        event_channel: &EventChannel
    ) -> Result<(), IOContextError> {
        Ok(self.inner.submitter().register_eventfd(
            event_channel.as_raw_fd()
        ).map_err(
            |e| IOContextError::FailedToRegisterEventChannel { source: e }
        )?)
    }
}

impl AsRawFd for IOContext {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}