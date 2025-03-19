use thiserror::Error;
use crate::events::channel::{
    EventChannelError, 
    EventSender
};
use super::Message;

#[derive(Debug, Error)]
pub enum WorkSenderError {
    #[error("Failed to send message: {0}")]
    SendError(#[from] crossbeam_channel::SendError<Message>),

    #[error("Failed to signal message: {0}")]
    SignalError(#[from] EventChannelError),
}

#[derive(Debug, Clone)]
pub struct WorkSender {
    sender: crossbeam_channel::Sender<Message>,
    message_signaler: EventSender,
}

impl WorkSender {
    pub fn new(sender: crossbeam_channel::Sender<Message>, message_signaler: EventSender) -> Self {
        Self { sender, message_signaler }
    }

    pub fn send_message(&self, msg: Message) -> Result<(), WorkSenderError> {
        self.sender.send(msg)?;
        self.message_signaler.signal()?;
        Ok(())
    }
}