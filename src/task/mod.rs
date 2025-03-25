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

pub use executor::Executor;
pub use waker::TaskWaker;
pub use id::TaskId;

pub struct Task {
    future: Pin<Box<dyn Future<Output = ()> + Send>>,
    pub id: TaskId,
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
}