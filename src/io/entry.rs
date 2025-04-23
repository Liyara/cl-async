

use crate::Key;

use super::{completion::IoMultiCompletionResult, operation::IoDuration, IoCompletionState, IoOperation}; 

pub struct IoEntry {
    pub op: IoOperation,
    pub key: Key,
}

impl IoEntry {

    pub fn as_uring_entry(&mut self) -> io_uring::squeue::Entry {
        self.op.as_uring_entry(self.key)
    }

    pub fn complete(mut self, result: i32) -> IoCompletionState {
        let completion_result = self.op.complete(result);
        match self.op.duration() {
            IoDuration::Zero => {
                IoCompletionState::Completed
            },
            IoDuration::Single => {
                IoCompletionState::Single(completion_result)
            },
            IoDuration::Persistent => {
                IoCompletionState::Multi(IoMultiCompletionResult::new(
                    completion_result,
                    self.op
                ))
            }
        }
    }
}



