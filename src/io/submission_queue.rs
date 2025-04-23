use crate::{
    key::KeyGenerator, 
    worker::{
        Message, 
        WorkSender
    }, 
    Key
};

use super::{IoOperation, IoSubmission, IoSubmissionError, IoSubmissionResult};

#[derive(Clone)]
pub struct IoSubmissionQueue {
    sender: WorkSender,
    generator: KeyGenerator,
}

impl IoSubmissionQueue {

    pub fn new(sender: WorkSender) -> Self {
        Self {
            sender,
            generator: KeyGenerator::new(0),
        }
    }

    pub fn submit(
        &self, 
        op: IoOperation,
        waker: Option<std::task::Waker>
    ) -> IoSubmissionResult<Key> {
        let key = self.generator.get();
        
        let submission = IoSubmission {
            op,
            key,
            waker: match waker {
                Some(ref w) => Some(w.clone()),
                None => None,
            },
        };

        self.sender.send_message(Message::SubmitIO(submission)).map_err(
            |e| {
                IoSubmissionError::FailedToSendOperation(e)
            }
        )?;

        Ok(key)
    }

    pub fn submit_multiple(
        &self, 
        op: Vec<IoOperation>, 
        waker: Option<std::task::Waker>
    ) -> IoSubmissionResult<Vec<Key>> {
        let mut keys = Vec::with_capacity(op.len());
        let mut entries = Vec::with_capacity(op.len());
        
        for op in op {
            let key = self.generator.get();
            entries.push(IoSubmission {
                op,
                key,
                waker: match waker {
                    Some(ref w) => Some(w.clone()),
                    None => None,
                },
            });
            keys.push(key);
        }
        
        self.sender.send_message(Message::SubmitIOMulti(entries)).map_err(
            |e| {
                IoSubmissionError::FailedToSendOperation(e)
            }
        )?;

        Ok(keys)
    }
}