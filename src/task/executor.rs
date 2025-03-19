use rustc_hash::FxHashMap;
use thiserror::Error;
use crate::worker::WorkSender;

use super::{
    Task, 
    TaskId,
    TaskWaker
};

#[derive(Debug, Error)]
pub enum ExecutorError {}

pub struct Executor {
    ready_tasks: crossbeam_deque::Worker<Task>,
    running_tasks: FxHashMap<TaskId, Task>,
    waker_cache: FxHashMap<TaskId, std::task::Waker>,
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

    pub fn run_ready_tasks(&mut self) {
        while let Some(task) = self.ready_tasks.pop() {
            self.run_task(task);
        }
    }

    pub fn wake_task(&mut self, task_id: TaskId) {

        if let Some(task) = self.running_tasks.remove(&task_id) {
            self.ready_tasks.push(task);
        }
    }

    pub fn has_ready_tasks(&self) -> bool {
        !self.ready_tasks.is_empty()
    }

    pub fn has_running_tasks(&self) -> bool {
        !self.running_tasks.is_empty()
    }

    fn run_task(&mut self, mut task: Task) {
        let task_id = task.id;
        
        let waker = self.waker_cache.entry(task_id).or_insert_with(|| 
            TaskWaker::new(task_id, self.tx.clone())
        );

        let mut context = std::task::Context::from_waker(waker);

        match task.poll(&mut context) {
            std::task::Poll::Ready(()) => {
                self.waker_cache.remove(&task_id);
            },
            std::task::Poll::Pending => {
                self.running_tasks.insert(task_id, task);
            },
        }
    }
}