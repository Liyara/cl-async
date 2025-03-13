use std::{sync::Arc, task::{Context, Poll, Wake, Waker}};
use log::error;
use rustc_hash::FxHashMap;
use thiserror::Error;

use crate::{message::Message, task::{Task, TaskId}, worker::{WorkSender, WorkerError}};

#[derive(Debug, Error)]
pub enum ExecutorError {

    #[error("Task {0} not found")]
    TaskNotFound(TaskId),

    #[error("Failed to send Continue message to worker ({source}")]
    FailedToSendContinue {
        #[source]
        source: Box<WorkerError>,
    },
}

pub struct Executor {
    ready_tasks: crossbeam_deque::Worker<Task>,
    running_tasks: FxHashMap<TaskId, Task>,
    waker_cache: FxHashMap<TaskId, Waker>,
    tx: WorkSender,
}

impl Executor {
    pub fn new(tx: WorkSender) -> Self {
        Self {
            ready_tasks: crossbeam_deque::Worker::new_fifo(),
            running_tasks: FxHashMap::default(),
            waker_cache: FxHashMap::default(),
            tx,
        }
    }

    pub fn spawn(&mut self, task: Task) {
        self.ready_tasks.push(task);
    }

    pub fn run_ready_tasks(&mut self) -> Result<(), ExecutorError> {
        while let Some(task) = self.ready_tasks.pop() {
            self.run_task(task)?;
        }
        Ok(())
    }

    pub fn wake_task(&mut self, task_id: TaskId) -> Result<(), ExecutorError> {
        let task = self.running_tasks.remove(&task_id).ok_or(
            ExecutorError::TaskNotFound(task_id),
        )?;

        self.ready_tasks.push(task);
        Ok(())
    }

    pub fn clear_cache_for(&mut self, task_id: TaskId) -> Result<(), ExecutorError> {
        self.waker_cache.remove(&task_id).ok_or(
            ExecutorError::TaskNotFound(task_id),
        )?;
        Ok(())
    }

    pub fn stealer(&self) -> crossbeam_deque::Stealer<Task> {
        self.ready_tasks.stealer()
    }

    pub fn has_ready_tasks(&self) -> bool {
        !self.ready_tasks.is_empty()
    }

    pub fn has_running_tasks(&self) -> bool {
        !self.running_tasks.is_empty()
    }

    fn run_task(&mut self, mut task: Task) -> Result<(), ExecutorError> {
        let task_id = task.id;
        
        let waker = self.waker_cache.entry(task_id).or_insert_with(|| 
            TaskWaker::new(task_id, self.tx.clone())
        );

        let mut context = Context::from_waker(waker);

        match task.poll(&mut context) {
            Poll::Ready(()) => {
                self.waker_cache.remove(&task_id);
            },
            Poll::Pending => {
                self.running_tasks.insert(task_id, task);
            },
        }

        Ok(())
    }
}

struct TaskWaker {
    task_id: TaskId,
    tx: WorkSender,
}

impl TaskWaker {

    fn new(task_id: TaskId, tx: WorkSender) -> Waker {
        Waker::from(Arc::new(TaskWaker {
            task_id,
            tx,
        }))
    } 

    fn wake_task(&self) -> Result<(), ExecutorError> {

        match self.tx.send_message(Message::WakeTask(self.task_id)) {
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