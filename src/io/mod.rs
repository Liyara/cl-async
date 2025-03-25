use crate::Key;

pub (crate) mod context;
pub (crate) mod submission;
pub (crate) mod completion;
pub (crate) mod operation;
pub mod fs;

pub (crate) use context::IOContext;
pub (crate) use submission::IOSubmissionQueue;
pub (crate) use completion::IOCompletionQueue;
pub (crate) use completion::IOCompletion;
pub (crate) use completion::IOReadCompletion;
pub (crate) use completion::IOWriteCompletion;

pub use operation::IOOperation;

#[derive(Debug, Clone, Copy)]
pub enum IOType {
    Read,
    Write,
}

pub struct IOEntry {
    op: IOOperation,
    key: Key,
    waker: Option<std::task::Waker>,
}


