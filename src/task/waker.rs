use std::sync::Arc;

use thiserror::Error;
use crate::worker::{
    work_sender::WorkSenderError, 
    Message, 
    WorkSender
};
use super::TaskId;

#[derive(Debug, Error)]
pub enum WakerError {
    #[error("Failed to send message: {0}")]
    SendError(#[from] WorkSenderError),
}

pub struct TaskWaker {
    task_id: TaskId,
    tx: WorkSender,
}

impl TaskWaker {

    pub fn new(task_id: TaskId, tx: WorkSender) -> std::task::Waker {
        std::task::Waker::from(Arc::new(TaskWaker {
            task_id,
            tx,
        }))
    } 

    fn wake_task(&self) -> Result<(), WakerError> {
        Ok(self.tx.send_message(Message::WakeTask(self.task_id))?)
    }
}

impl std::task::Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_task().unwrap_or_else(|e| log::error!("{}", e));
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task().unwrap_or_else(|e| log::error!("{}", e));
    }
}