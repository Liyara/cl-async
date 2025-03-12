use std::{sync::Arc, task::{Context, Poll, Wake, Waker}};

use crossbeam_queue::ArrayQueue;
use log::error;
use rustc_hash::FxHashMap;
use thiserror::Error;

use crate::{message::Message, task::{Task, TaskId}, worker::{WorkSender, WorkerError}};

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("Duplicate task ID {0}")]
    DuplicateTaskId(TaskId),

    #[error("Queue full when attempting to push task with ID {0}")]
    QueueFull(TaskId),

    #[error("Failed to send Continue message to worker ({source}")]
    FailedToSendContinue {
        #[source]
        source: Box<WorkerError>,
    },
}


pub struct Executor {
    tasks: FxHashMap<TaskId, Task>,
    task_queue: Arc<ArrayQueue<TaskId>>,
    tx: WorkSender,
    waker_cache: FxHashMap<TaskId, Waker>,
}

impl Executor {
    pub (crate) fn new(capacity: usize, tx: WorkSender) -> Self {
        Self {
            tasks: FxHashMap::default(),
            task_queue: Arc::new(ArrayQueue::new(capacity)),
            tx,
            waker_cache: FxHashMap::default(),
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
    tx: WorkSender,
    task_queue: Arc<ArrayQueue<TaskId>>,
}

impl TaskWaker {

    fn new(task_id: TaskId, task_queue: Arc<ArrayQueue<TaskId>>, tx: WorkSender) -> Waker {
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

        match self.tx.send_message(Message::Continue) {
            Ok(_) => {}
            Err(e) => return Err(ExecutorError::FailedToSendContinue { source: Box::new(e) }),
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