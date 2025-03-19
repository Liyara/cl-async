use thiserror::Error;
use super::{IOEntry, IOOperation};

use crate::{
    key::KeyGenerator, 
    worker::{
        work_sender::WorkSenderError, 
        Message, 
        WorkSender
    }, 
    Key
};

#[derive(Debug, Error)]
pub enum IOSubmissionError {
    #[error("Failed to submit IO operation: {0}")]
    SubmissionError(#[from] WorkSenderError),
}

#[derive(Clone)]
pub struct IOSubmissionQueue {
    sender: WorkSender,
    generator: KeyGenerator,
}

impl IOSubmissionQueue {

    pub fn new(sender: WorkSender) -> Self {
        Self {
            sender,
            generator: KeyGenerator::new(0),
        }
    }

    pub fn submit(&self, op: IOOperation, waker: Option<std::task::Waker>) -> Result<Key, IOSubmissionError> {
        let key = self.generator.get();
        let entry = IOEntry {
            op,
            key,
            waker: match waker {
                Some(ref w) => Some(w.clone()),
                None => None,
            },
        };
        self.sender.send_message(Message::SubmitIOEntry(entry))?;
        Ok(key)
    }

    pub fn submit_multiple(&self, op: Vec<IOOperation>, waker: Option<std::task::Waker>) -> Result<Vec<Key>, IOSubmissionError> {
        let mut keys = Vec::with_capacity(op.len());
        let mut entries = Vec::with_capacity(op.len());
        for op in op {
            let key = self.generator.get();
            entries.push(IOEntry {
                op,
                key,
                waker: match waker {
                    Some(ref w) => Some(w.clone()),
                    None => None,
                },
            });
            keys.push(key);
        }
        self.sender.send_message(Message::SubmitIOEntries(entries))?;
        Ok(keys)
    }
}