use std::os::fd::{
    AsRawFd, 
    RawFd
};

use crate::{
    events::EventChannel, 
    Key
};

use super::{IoCompletionQueue, IoError, IoSubmission, IoSubmissionError, IoSubmissionResult};

pub struct IoContext {
    inner: io_uring::IoUring,
    completion_queue: IoCompletionQueue,
}

impl IoContext {

    pub fn new(entries: u32) -> Result<Self, IoError> {
        Ok(Self {
            inner: io_uring::IoUring::new(entries).map_err(
                |e| IoError::FailedToCreateContext { source: e }
            )?,
            completion_queue: IoCompletionQueue::new()
        })
    }

    fn try_push_entry(
        &mut self, 
        entry: &mut io_uring::squeue::Entry
    ) -> IoSubmissionResult<()> {
        Ok(unsafe { self.inner.submission().push(&entry)?; })
    }

    pub fn prepare_submission(&mut self, submission: IoSubmission) -> Option<IoSubmission> {

        let key = submission.key;
        let (mut entry, waker) = submission.split();
        let mut uring_entry = entry.as_uring_entry();
        if let Err(_) = self.try_push_entry(&mut uring_entry) {
            let submission = IoSubmission {
                key,
                op: entry.op,
                waker
            };
            return Some(submission);
        }
        self.completion_queue.add_entry(key, entry, waker);
        
        None
    }
        

    pub fn submit(&self) -> Result<usize, IoError> {
        Ok(self.inner.submit().map_err(
            |e| IoSubmissionError::FailedToSubmitEntries { source: e }
        )?)
    }

    pub fn blocking_submit(&self) -> Result<usize, IoError> {
        Ok(self.inner.submit_and_wait(self.completion_queue.len()).map_err(
            |e| IoSubmissionError::FailedToSubmitEntries { source: e }
        )?)
    }

    pub fn complete(&mut self) {
        let mut cqueue = self.inner.completion();
        cqueue.sync();

        while let Some(item) = cqueue.next() {
            let result_code = item.result();
            let key = Key::new(item.user_data());

            self.completion_queue.complete(key, result_code);
        }
    }

    pub fn completion(&self) -> &IoCompletionQueue {
        &self.completion_queue
    }

    pub fn register_event_channel(
        &mut self, 
        event_channel: &EventChannel
    ) -> Result<(), IoError> {
        Ok(self.inner.submitter().register_eventfd(
            event_channel.as_raw_fd()
        ).map_err(
            |e| IoSubmissionError::FailedToRegisterEventChannel { source: e }
        )?)
    }
}

impl AsRawFd for IoContext {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}