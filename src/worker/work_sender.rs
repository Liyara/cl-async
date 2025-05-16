use crossbeam_channel::SendError;
use thiserror::Error;
use crate::events::channel::EventChannelSender;
use super::Message;

#[derive(Debug, Error)]
#[error("Failed to send message to worker channel: {source}")]
pub struct SendToWorkerChannelError {

    #[from]
    source: SendError<Message>
}

impl SendToWorkerChannelError {
    pub fn into_message(self) -> Message {
        self.source.into_inner()
    }

    pub fn as_message(&self) -> &Message {
        &self.source.0
    }
}

#[derive(Debug, Clone)]
pub struct WorkSender {
    sender: crossbeam_channel::Sender<Message>,
    message_signaler: EventChannelSender,
}

impl WorkSender {
    pub fn new(sender: crossbeam_channel::Sender<Message>, message_signaler: EventChannelSender) -> Self {
        Self { sender, message_signaler }
    }

    pub fn send_message(&self, msg: Message) -> Result<(), SendToWorkerChannelError> {
        self.sender.send(msg)?;
        if let Err(_) = self.message_signaler.signal() {
            // Might be benign, just retry first
            if let Err(e) = self.message_signaler.signal() {
                // Something is likely wrong with the signaler.
                warn!("cl-async: Failed to signal message worker message: {e}");
                if let Err(e) = self.sender.send(
                    Message::RepairMessageChannel
                ) {
                    error!("cl-async: Failed to send channel repair message: {e}");
                }
            }
        }
        Ok(())
    }
}