use std::ffi::NulError;

use bytes::Bytes;
use bytes::BytesMut;
use failure::IoFailure;
use message::PrepareControlMessageError;
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
mod failure;

pub use submission::IoSubmission;
pub use submission_queue::IoSubmissionQueue;

pub use entry::IoEntry;

pub use context::IoContext;
pub use context::IoContextError;

pub use operation::IoType;
pub use operation::IoOperation;
pub use operation::data as operation_data;

pub use completion_queue::IoCompletionQueue;

pub use completion::IoCompletion;
pub use completion::IoCompletionResult;
pub use completion::IoCompletionState;
pub use completion::data as completion_data;

pub use owned_fd_async::OwnedFdAsync;

pub use message::IoControlMessage;
pub use message::IoControlMessageLevel;
pub use message::PacketInfo;
pub use message::IpV6TrafficClass;
pub use message::Credentials;
pub use message::SocketLevelType;
pub use message::IpV4LevelType;
pub use message::IpV6LevelType;
pub use message::TlsLevelType;
pub use message::TlsRecordType;
pub use message::TlsAlertLevel;
pub use message::TlsAlertDescription;
pub use message::TlsAlert;
pub use message::IoSendMessage;
pub use message::IoRecvMessage;
pub use message::IoMessage;
pub use message::PendingIoMessage;
pub use message::PreparedIoMessage;

pub use buffers::IoInputBuffer;
pub use buffers::IoOutputBuffer;
pub use buffers::GenerateIoVecs;

use crate::pool::WorkerDispatchError;
use crate::OsError;

// These errors occur before an operation leaves the calling thread
#[derive(Debug, Error)]
pub enum IoSubmissionError {

    #[error("Empty input buffer")]
    EmptyInputBuffer(Bytes),

    #[error("Empty vectored input buffer")]
    EmptyVectoredInputBuffer(Vec<Bytes>),

    #[error("vectored input buffer contains empty buffer at index {i}")]
    EmptyVectoredInputBufferEntry {
        i: usize,
        buffers: Vec<Bytes>,
    },

    #[error("Non-contiguous input buffer")]
    NonContiguousInputBuffer(Bytes),
    
    #[error("Empty output buffer")]
    EmptyOutputBuffer(BytesMut),

    #[error("Empty vectored output buffer")]
    EmptyVectoredOutputBuffer(Vec<BytesMut>),

    #[error("vectored output buffer contains empty buffer at index {i}")]
    EmptyVectoredOutputBufferEntry {
        i: usize,
        buffers: Vec<BytesMut>,
    },

    #[error("Unsupported address family: {0}")]
    UnsupportedAddressFamily(libc::sa_family_t),

    #[error("A string was passed with interior null bytes ({0})")]
    StringWithInteriorNullBytes(#[from] NulError),

    #[error("Could not locate any writable memory when generating io vecs.")]
    NoWritableMemory,

    #[error("Too many buffers were passed to the IO operation, {requested} > {max}")]
    TooManyIoVecs {
        requested: usize,
        max: usize,
    },

    #[error("Failed to prepare control messages: {0}")]
    FailedToPrepareControlMessages(#[source] PrepareControlMessageError),

    #[error("Failed to dispatch operation to worker thread")]
    FailedToDispatchOperation(#[from] WorkerDispatchError),
}

// These errors occur after an operation has been submitted to the kernel
#[derive(Debug, Error)]
#[error("Error occured during IO operation: {os_error}")]
pub struct IoOperationError {

    failure: IoFailure,

    #[source]
    os_error: OsError
}

// These errors occur after a completion has been received from the kernel
#[derive(Debug, Error)]
pub enum IoProcessingError {
    #[error("Got unexpected completion result: {0}")]
    UnexpectedCompletionResult(String),
}

// General wrapper for all IO errors
#[derive(Debug, Error)]
pub enum IoError {
    #[error("IO Submission error: {0}")]
    Submission(#[from] IoSubmissionError),

    #[error("IO Operation error: {0}")]
    Operation(#[from] IoOperationError),

    #[error("IO Context error: {0}")]
    Context(#[from] IoContextError),

    #[error("IO Processing error: {0}")]
    Processing(#[from] IoProcessingError),
}

impl IoError {
    pub fn as_os_error(&self) -> Option<&OsError> {
        match self {
            IoError::Operation(e) => Some(&e.os_error),
            _ => None,
        }
    }
}