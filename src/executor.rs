use std::{collections::BTreeMap, sync::{mpsc::Sender, Arc}, task::{Context, Poll, Wake, Waker}};

use crossbeam_queue::ArrayQueue;
use log::error;
use thiserror::Error;

use crate::{message::Message, task::{Task, TaskId}};

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("Duplicate task ID {0}")]
    DuplicateTaskId(TaskId),

    #[error("Queue full when attempting to push task with ID {0}")]
    QueueFull(TaskId),

    #[error("Failed to send Continue message to worker")]
    SendContinue(#[from] std::sync::mpsc::SendError<Message>),
}


pub struct Executor {
    tasks: BTreeMap<TaskId, Task>,
    task_queue: Arc<ArrayQueue<TaskId>>,
    tx: Sender<Message>,
    waker_cache: BTreeMap<TaskId, Waker>,
}

impl Executor {
    pub (crate) fn new(capacity: usize, tx: Sender<Message>) -> Self {
        Self {
            tasks: BTreeMap::new(),
            task_queue: Arc::new(ArrayQueue::new(capacity)),
            tx,
            waker_cache: BTreeMap::new(),
        }
    }

    pub (crate) fn spawn(&mut self, task: Task) -> Result<(), ExecutorError> {
        let task_id = task.id;
        if self.tasks.insert(task_id, task).is_some() {
            return Err(ExecutorError::DuplicateTaskId(task_id));
        }
        self.task_queue.push(task_id).map_err(|t| ExecutorError::QueueFull(t))?;

        Ok(())
    }

    pub (crate) fn run_ready_tasks(&mut self) {

        while let Some(task_id) = self.task_queue.pop() {
            let task = match self.tasks.get_mut(&task_id) {
                Some(task) => task,
                None => continue,
            };
            let waker = self.waker_cache
                .entry(task_id)
                .or_insert_with(|| TaskWaker::new(task_id, self.task_queue.clone(), self.tx.clone()));
            let mut context = Context::from_waker(waker);
            match task.poll(&mut context) {
                Poll::Ready(()) => {
                    self.tasks.remove(&task_id);
                    self.waker_cache.remove(&task_id);
                }
                Poll::Pending => {}
            }
        }
    }

    pub (crate) fn has_tasks(&self) -> bool {
        !self.tasks.is_empty()
    }

    pub (crate) fn has_ready_tasks(&self) -> bool {
        !self.task_queue.is_empty()
    }

}

struct TaskWaker {
    task_id: TaskId,
    tx: Sender<Message>,
    task_queue: Arc<ArrayQueue<TaskId>>,
}

impl TaskWaker {

    fn new(task_id: TaskId, task_queue: Arc<ArrayQueue<TaskId>>, tx: Sender<Message>) -> Waker {
        Waker::from(Arc::new(TaskWaker {
            task_id,
            tx,
            task_queue,
        }))
    }

    fn wake_task(&self) -> Result<(), ExecutorError> {
        match self.task_queue.push(self.task_id) {
            Ok(_) => {}
            Err(id) => return Err(ExecutorError::QueueFull(id)),
        }

        match self.tx.send(Message::Continue) {
            Ok(_) => {}
            Err(e) => return Err(ExecutorError::SendContinue(e)),
        }

        Ok(())
    }
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_task().unwrap_or_else(|e| error!("{}", e));
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task().unwrap_or_else(|e| error!("{}", e));
    }
}