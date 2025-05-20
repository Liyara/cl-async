use std::ffi::NulError;

use buffers::RecvMsgBuffersRefs;
use bytes::BytesMut;
use enum_dispatch::enum_dispatch;
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
pub use buffers::InputBufferSubmissionError;
pub use buffers::InputBufferVecSubmissionError;
pub use buffers::OutputBufferSubmissionError;
pub use buffers::OutputBufferVecSubmissionError;
pub use buffers::EmptyIoOutputBufferError;
pub use buffers::InvalidIoInputBufferErrorKind;
pub use buffers::EmptyVectoredInputBufferError;
pub use buffers::EmptyVectoredOutputBufferError;
pub use buffers::InvalidIoInputBufferError;
pub use buffers::InvalidIoVecError;
pub use buffers::RecvMsgBuffers;
pub use buffers::RecvMsgSubmissionError;
pub use buffers::EmptySingleVectoredInputBufferError;

use crate::pool::WorkerDispatchError;
use crate::worker::Message;
use crate::OsError;

// Traits for retrieving data from errors.

#[enum_dispatch]
pub trait IoBytesMutRecovery {
    fn as_bytes_mut(&self) -> Option<&BytesMut>;
    fn into_bytes_mut(self) -> Option<BytesMut>;
}

#[enum_dispatch]
pub trait IoBytesMutVecRecovery {
    fn as_vec(&self) -> Option<&Vec<BytesMut>>;
    fn into_vec(self) -> Option<Vec<BytesMut>>;
}

#[enum_dispatch]
pub trait IoRecvMessageRecovery {
    fn as_recvmsg_buffers(&self) -> Option<RecvMsgBuffersRefs>;
    fn into_recvmsg_buffers(self) -> Option<RecvMsgBuffers>;
}

// These errors occur before an operation leaves the calling thread
#[derive(Debug, Error)]
pub enum IoSubmissionError {

    // Data parsing errors
    #[error("Unsupported address family: {0}")]
    UnsupportedAddressFamily(libc::sa_family_t),

    #[error("A string was passed with interior null bytes ({0})")]
    StringWithInteriorNullBytes(#[from] NulError),

    #[error("Failed to prepare control messages: {0}")]
    FailedToPrepareControlMessages(#[source] PrepareControlMessageError),


    // Worker dispatch errors
    #[error("Failed to dispatch operation to worker thread")]
    FailedToDispatchOperation(#[from] WorkerDispatchError<IoOperation>),


    // Buffer submission errors
    #[error("Failed to input buffer for operation: {0}")]
    InputBufferSubmissionError(#[from] InputBufferSubmissionError),

    #[error("Failed to submit input buffers for operation: {0}")]
    InputBufferVecSubmissionError(#[from] InputBufferVecSubmissionError),

    #[error("Failed to submit output buffer for operation: {0}")]
    OutputBufferSubmissionError(#[from] OutputBufferSubmissionError),

    #[error("Failed to submit output buffers for operation: {0}")]
    OutputBufferVecSubmissionError(#[from] OutputBufferVecSubmissionError),

    #[error("Failed to submit recvmsg buffers for operation: {0}")]
    RecvMsgBuffersSubmissionError(#[from] RecvMsgSubmissionError),

    #[error("A buffer generated invalod iovecs: {0}")]
    InvalidIoInputBufferError(#[from] InvalidIoVecError),

}

impl From<EmptyVectoredOutputBufferError> for IoSubmissionError {
    fn from(e: EmptyVectoredOutputBufferError) -> Self {
        IoSubmissionError::OutputBufferVecSubmissionError(
            OutputBufferVecSubmissionError::InvalidOutputBuffer(e)
        )
    }
}

impl From<EmptyIoOutputBufferError> for IoSubmissionError {
    fn from(e: EmptyIoOutputBufferError) -> Self {
        IoSubmissionError::OutputBufferSubmissionError(
            OutputBufferSubmissionError::InvalidOutputBuffer(e)
        )
    }
}

impl From<EmptyVectoredInputBufferError> for IoSubmissionError {
    fn from(e: EmptyVectoredInputBufferError) -> Self {
        IoSubmissionError::InputBufferVecSubmissionError(
            InputBufferVecSubmissionError::InvalidIoVecInputBuffer(e)
        )
    }
}

impl From<InvalidIoInputBufferError> for IoSubmissionError {
    fn from(e: InvalidIoInputBufferError) -> Self {
        IoSubmissionError::InputBufferSubmissionError(
            InputBufferSubmissionError::InvalidInputBuffer(e)
        )
    }
}

impl From<EmptySingleVectoredInputBufferError> for IoSubmissionError {
    fn from(e: EmptySingleVectoredInputBufferError) -> Self {
        IoSubmissionError::InputBufferSubmissionError(
            InputBufferSubmissionError::InvalidIoVecInputBuffer(e)
        )
    }
}

impl IoBytesMutRecovery for IoSubmissionError {
    fn as_bytes_mut(&self) -> Option<&BytesMut> {
        match self {
            IoSubmissionError::FailedToDispatchOperation(e) => match e {
                WorkerDispatchError::FailedToFindWorker {payload, .. } => {
                    payload.as_bytes_mut()
                }
                WorkerDispatchError::FailedToSendMessage(send_error) => {
                    match send_error.as_message() {
                        Message::SubmitIO(submission) => {
                            submission.op.as_bytes_mut()
                        },
                        _ => None,
                    }
                }  
            },
            IoSubmissionError::OutputBufferSubmissionError(output_buffer_submission_error) => {
                match output_buffer_submission_error {
                    OutputBufferSubmissionError::InvalidOutputBuffer(e) => Some(&e.buffer),
                    OutputBufferSubmissionError::InvalidIoVec { buffer, ..} => Some(buffer),
                }
            },
            IoSubmissionError::RecvMsgBuffersSubmissionError(recv_msg_submission_error) => {
                recv_msg_submission_error.as_buffers().control()
            }
            _ => None,
        }
    }
    
    fn into_bytes_mut(self) -> Option<BytesMut> {
        match self {
            IoSubmissionError::FailedToDispatchOperation(e) => match e {
                WorkerDispatchError::FailedToFindWorker {payload, .. } => {
                    payload.into_bytes_mut()
                }
                WorkerDispatchError::FailedToSendMessage(send_error) => {
                    match send_error.into_message() {
                        Message::SubmitIO(submission) => {
                            submission.op.into_bytes_mut()
                        },
                        _ => None,
                    }
                }  
            },
            IoSubmissionError::OutputBufferSubmissionError(output_buffer_submission_error) => {
                match output_buffer_submission_error {
                    OutputBufferSubmissionError::InvalidOutputBuffer(e) => Some(e.buffer),
                    OutputBufferSubmissionError::InvalidIoVec { buffer, ..} => Some(buffer),
                }
            },
            IoSubmissionError::RecvMsgBuffersSubmissionError(recv_msg_submission_error) => {
                recv_msg_submission_error.into_buffers().take_control()
            }
            _ => None,
        }
    }
}

impl IoBytesMutVecRecovery for IoSubmissionError {
    fn as_vec(&self) -> Option<&Vec<BytesMut>> {
        match self {
            IoSubmissionError::FailedToDispatchOperation(e) => match e {
                WorkerDispatchError::FailedToFindWorker {payload, .. } => {
                    payload.as_vec()
                }
                WorkerDispatchError::FailedToSendMessage(send_error) => {
                    match send_error.as_message() {
                        Message::SubmitIO(submission) => {
                            submission.op.as_vec()
                        },
                        _ => None,
                    }
                }  
            },
            IoSubmissionError::OutputBufferVecSubmissionError(output_buffer_vec_submission_error) => {
                match output_buffer_vec_submission_error {
                    OutputBufferVecSubmissionError::InvalidIoVec { buffer, ..} => Some(buffer),
                    OutputBufferVecSubmissionError::InvalidOutputBuffer(e) => Some(&e.buffer),
                }
            },
            IoSubmissionError::RecvMsgBuffersSubmissionError(recv_msg_submission_error) => {
                recv_msg_submission_error.as_buffers().data()
            }
            _ => None,
        }
    }

    fn into_vec(self) -> Option<Vec<BytesMut>> {
        match self {
            IoSubmissionError::FailedToDispatchOperation(e) => match e {
                WorkerDispatchError::FailedToFindWorker {payload, .. } => {
                    payload.into_vec()
                }
                WorkerDispatchError::FailedToSendMessage(send_error) => {
                    match send_error.into_message() {
                        Message::SubmitIO(submission) => {
                            submission.op.into_vec()
                        },
                        _ => None,
                    }
                }  
            },
            IoSubmissionError::OutputBufferVecSubmissionError(output_buffer_vec_submission_error) => {
                match output_buffer_vec_submission_error {
                    OutputBufferVecSubmissionError::InvalidIoVec { buffer, ..} => Some(buffer),
                    OutputBufferVecSubmissionError::InvalidOutputBuffer(e) => Some(e.buffer),
                }
            },
            IoSubmissionError::RecvMsgBuffersSubmissionError(recv_msg_submission_error) => {
                recv_msg_submission_error.into_buffers().take_data()
            }
            _ => None,
        }
    }
}

impl IoRecvMessageRecovery for IoSubmissionError {
    fn as_recvmsg_buffers(&self) -> Option<RecvMsgBuffersRefs> {
        match self {
            IoSubmissionError::FailedToDispatchOperation(e) => match e {
                WorkerDispatchError::FailedToFindWorker {payload, .. } => {
                    payload.as_recvmsg_buffers()
                }
                WorkerDispatchError::FailedToSendMessage(send_error) => {
                    match send_error.as_message() {
                        Message::SubmitIO(submission) => {
                            submission.op.as_recvmsg_buffers()
                        },
                        _ => None,
                    }
                }  
            },
            IoSubmissionError::RecvMsgBuffersSubmissionError(recv_msg_submission_error) => {
                Some(recv_msg_submission_error.as_buffers().refs())
            }
            _ => None,
        }
    }

    fn into_recvmsg_buffers(self) -> Option<RecvMsgBuffers> {
        match self {
            IoSubmissionError::FailedToDispatchOperation(e) => match e {
                WorkerDispatchError::FailedToFindWorker {payload, .. } => {
                    payload.into_recvmsg_buffers()
                }
                WorkerDispatchError::FailedToSendMessage(send_error) => {
                    match send_error.into_message() {
                        Message::SubmitIO(submission) => {
                            submission.op.into_recvmsg_buffers()
                        },
                        _ => None,
                    }
                }  
            },
            IoSubmissionError::RecvMsgBuffersSubmissionError(recv_msg_submission_error) => {
                Some(recv_msg_submission_error.into_buffers())
            }
            _ => None,
        }
    }
}

// These errors occur after an operation has been submitted to the kernel
#[derive(Debug, Error)]
#[error("Error occured during IO operation: {os_error}")]
pub struct IoOperationError {

    failure: IoFailure,

    #[source]
    os_error: OsError
}

impl IoBytesMutRecovery for IoOperationError {

    fn as_bytes_mut(&self) -> Option<&BytesMut> {
        match &self.failure {
            IoFailure::Read(data) => Some(&data.buffer),
            _ => None,
        }
    }

    fn into_bytes_mut(self) -> Option<BytesMut> {
        match self.failure {
            IoFailure::Read(data) => Some(data.buffer),
            _ => None,
        }
    }
}

impl IoBytesMutVecRecovery for IoOperationError {
    fn as_vec(&self) -> Option<&Vec<BytesMut>> {
        match &self.failure {
            IoFailure::MultiRead(data) => Some(&data.buffers),
            _ => None,
        }
    }

    fn into_vec(self) -> Option<Vec<BytesMut>> {
        match self.failure {
            IoFailure::MultiRead(data) => Some(data.buffers),
            _ => None,
        }
    }
}

impl IoRecvMessageRecovery for IoOperationError {
    fn as_recvmsg_buffers(&self) -> Option<RecvMsgBuffersRefs> {
        match &self.failure {
            IoFailure::Msg(data) => Some(data.buffers.refs()),
            _ => None,
        }
    }

    fn into_recvmsg_buffers(self) -> Option<RecvMsgBuffers> {
        match self.failure {
            IoFailure::Msg(data) => Some(data.buffers),
            _ => None,
        }
    }
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

    pub fn into_os_error(self) -> Option<OsError> {
        match self {
            IoError::Operation(e) => Some(e.os_error),
            _ => None,
        }
    }
   
}

impl IoBytesMutRecovery for IoError {

    fn as_bytes_mut(&self) -> Option<&BytesMut> {
        match self {
            IoError::Submission(e) => e.as_bytes_mut(),
            IoError::Operation(e) => e.as_bytes_mut(),
            IoError::Processing(_) => None,
        }
    }

    fn into_bytes_mut(self) -> Option<BytesMut> {
        match self {
            IoError::Submission(e) => e.into_bytes_mut(),
            IoError::Operation(e) => e.into_bytes_mut(),
            IoError::Processing(_) => None,
        }
    }
}

impl IoBytesMutVecRecovery for IoError {

    fn as_vec(&self) -> Option<&Vec<BytesMut>> {
        match self {
            IoError::Submission(e) => e.as_vec(),
            IoError::Operation(e) => e.as_vec(),
            IoError::Processing(_) => None,
        }
    }

    fn into_vec(self) -> Option<Vec<BytesMut>> {
        match self {
            IoError::Submission(e) => e.into_vec(),
            IoError::Operation(e) => e.into_vec(),
            IoError::Processing(_) => None,
        }
    }
}

impl IoRecvMessageRecovery for IoError {

    fn as_recvmsg_buffers(&self) -> Option<RecvMsgBuffersRefs> {
        match self {
            IoError::Submission(e) => e.as_recvmsg_buffers(),
            IoError::Operation(e) => e.as_recvmsg_buffers(),
            IoError::Processing(_) => None,
        }
    }

    fn into_recvmsg_buffers(self) -> Option<RecvMsgBuffers> {
        match self {
            IoError::Submission(e) => e.into_recvmsg_buffers(),
            IoError::Operation(e) => e.into_recvmsg_buffers(),
            IoError::Processing(_) => None,
        }
    }
}
            