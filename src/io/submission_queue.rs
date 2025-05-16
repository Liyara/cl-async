use crate::{
    key::KeyGenerator, 
    worker::{
        work_sender::SendToWorkerChannelError, Message, WorkSender
    }, 
    Key
};

use super::{IoOperation, IoSubmission};

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
    ) -> Result<Key, SendToWorkerChannelError> {
        let key = self.generator.get();
        
        let submission = IoSubmission {
            op,
            key,
            waker: match waker {
                Some(ref w) => Some(w.clone()),
                None => None,
            },
        };

        let message = Message::SubmitIO(submission);

        self.sender.send_message(message)?;

        Ok(key)
    }
}