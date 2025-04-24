use std::path::PathBuf;

use thiserror::Error;

pub mod fs;

mod submission;
mod entry;
pub mod operation;
mod completion;
mod completion_queue;
mod context;
mod submission_queue;
mod owned_fd_async;
mod message;
mod buffers;

pub use submission::IoSubmission;
pub use submission_queue::IoSubmissionQueue;
pub use entry::IoEntry;
pub use completion_queue::IoCompletionQueue;
pub use context::IoContext;

pub use operation::IoType;
pub use operation::IoOperation;
pub use operation::data as operation_data;

pub use completion::IoCompletion;
pub use completion::IoCompletionResult;
pub use completion::IoCompletionState;
pub use completion::data as completion_data;

pub use owned_fd_async::OwnedFdAsync;

pub use message::IoMessage;
pub use message::IoControlMessage;
pub use message::IoControlMessageLevel;
pub use message::PacketInfo;
pub use message::IpV6TrafficClass;
pub use message::Credentials;
pub use message::IoMessageFuture;
pub use message::PreparedIoMessage;
pub use message::SocketLevelType;
pub use message::IpV4LevelType;
pub use message::IpV6LevelType;
pub use message::TlsLevelType;
pub use message::TlsRecordType;
pub use message::TlsAlertLevel;
pub use message::TlsAlertDescription;

pub use buffers::IoInputBuffer;
pub use buffers::IoOutputBuffer;
pub use buffers::IoBuffer;
pub use buffers::GenerateIoVecs;
pub use buffers::IoDoubleBuffer;
pub use buffers::IoDoubleInputBuffer;
pub use buffers::IoDoubleOutputBuffer;

use crate::net::NetworkError;
use crate::OsError;

#[derive(Debug, Error)]
pub enum IoSubmissionError {
    #[error("Failed to send operation to worker: {0}")]
    FailedToSendOperation(#[from] crate::worker::work_sender::WorkSenderError),

    #[error("Failed to submit IO operation request: {0}")]
    FailedToSubmitIoOperationRequest(String),

    #[error("Invalid String: {0}")]
    InvalidString(#[from] std::ffi::NulError),

    #[error("Invalid path: {0}")]
    InvalidPath(PathBuf),

    #[error("Failed to register event channel: {source}")]
    FailedToRegisterEventChannel { source: std::io::Error },

    #[error("Failed to push SQE: {0}")]
    FailedToPushSQE(#[from] io_uring::squeue::PushError),

    #[error("Failed to submit IO entries to kernel: {source}")]
    FailedToSubmitEntries { source: std::io::Error},

    #[error("Network Error: {0}")]
    Network(#[from] NetworkError),

    #[error("Empty buffer")]
    EmptyBuffer,

}

#[derive(Debug, Error)]
pub enum IoOperationError {
    #[error("IO Error: {0}")]
    Generic(#[from] std::io::Error),

    #[error("OS Error: {0}")]
    Os(#[from] OsError),

    #[error("Network Error: {0}")]
    Network(#[from] NetworkError),

    #[error("Invalid UTF-8: {0}")]
    InvalidFromUtf8(#[from] std::string::FromUtf8Error),

    #[error("Invalid UTF-8: {0}")]
    InvalidIntoUtf8C(#[from] std::ffi::IntoStringError),

    #[error("Invalid path: {0}")]
    InvalidPath(PathBuf),

    #[error("Expected output is empty")]
    NoData,

    #[error("Invalid pointer")]
    InvalidPtr,

    #[error("Buffer overflow: allowed {0} bytes, but got {1}")]
    BufferOverflow(usize, usize),

    #[error("Index out of bounds: {0}")]
    OutOfBounds(usize),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Unexpected completion poll result: {0}")]
    UnexpectedPollResult(String),
}

#[derive(Debug, Error)]
pub enum IoError {
    #[error("Submission error: {0}")]
    Submission(#[from] IoSubmissionError),

    #[error("Operation error: {0}")]
    Operation(#[from] IoOperationError),

    #[error("Failed to create IO Context: {source}")]
    FailedToCreateContext { source: std::io::Error },
}

pub type IoSubmissionResult<T> = std::result::Result<T, IoSubmissionError>;
//type IoOperationResult<T> = std::result::Result<T, IoOperationError>;
pub type IoResult<T> = std::result::Result<T, IoError>;