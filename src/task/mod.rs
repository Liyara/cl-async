use std::{
    pin::Pin, 
    task::{
        Context, 
        Poll
    }
};

pub mod executor;
pub mod id;
pub mod waker;
pub mod task_factory;

pub use executor::Executor;
pub use waker::TaskWaker;
pub use id::TaskId;
pub use task_factory::TaskFactory;

pub struct Task {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    pub id: TaskId,
}

impl std::fmt::Debug for Task {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Task")
            .field("id", &self.id)
            .finish()
    }
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + Send + 'static) -> Self {
        Self {
            future: Box::pin(future),
            id: TaskId::new(),
        }
    }

    pub fn poll(&mut self, cx: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(cx)
    }

    pub fn unwrap(self) -> impl Future<Output = ()> {
        self.future
    }

    pub fn into_local(self) -> LocalTask {
        LocalTask {
            future: self.future,
            id: self.id,
        }
    }
}

impl<T> From<T> for Task 
where
    T: Future<Output = ()> + Send + 'static
{
    fn from(future: T) -> Self {
        Task::new(future)
    }
}

pub struct LocalTask {
    future: Pin<Box<dyn Future<Output = ()>>>,
    pub id: TaskId,
}

impl LocalTask {
    pub fn new(future: impl Future<Output = ()> + 'static) -> Self {
        Self {
            future: Box::pin(future),
            id: TaskId::new(),
        }
    }

    pub fn poll(&mut self, cx: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(cx)
    }

    pub fn unwrap(self) -> impl Future<Output = ()> {
        self.future
    }
}

impl<T> From<T> for LocalTask 
where
    T: Future<Output = ()> + 'static
{
    fn from(future: T) -> Self {
        LocalTask::new(future)
    }
}

impl From<Task> for LocalTask {
    fn from(task: Task) -> Self {
        LocalTask {
            future: task.future,
            id: task.id,
        }
    }
}