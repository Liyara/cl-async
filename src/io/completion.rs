use std::collections::VecDeque;

use crate::OsError;

use super::{
    IoEntry, IoOperation, IoOperationError
};

pub mod data {

    use std::os::fd::RawFd;

    use bytes::{Bytes, BytesMut};

    use crate::io::{message::IoRecvMessage, operation_data::IoStatxMask};

    pub struct IoReadCompletion {
        pub buffer: BytesMut,
        pub bytes_read: usize,
    }

    pub struct IoWriteCompletion {
        pub bytes_written: usize,
    }

    pub struct IoMultiReadCompletion {
        pub buffers: Vec<BytesMut>,
        pub bytes_read: usize,
    }

    pub struct IoMsgCompletion {
        pub msg: IoRecvMessage
    }

    pub struct IoAcceptCompletion {
        pub fd: RawFd,
        pub address: Option<libc::sockaddr_storage>,
    }

    pub struct IoFileCompletion {
        pub fd: RawFd,
        pub path: Bytes,
    }

    pub struct IoStatxCompletion {
        pub stats: libc::statx,
        pub mask: IoStatxMask,
    }
}

pub enum IoCompletion {
    Success,
    Read(data::IoReadCompletion),
    MultiRead(data::IoMultiReadCompletion),
    Msg(data::IoMsgCompletion),
    Write(data::IoWriteCompletion),
    Accept(data::IoAcceptCompletion),
    File(data::IoFileCompletion),
    Stats(data::IoStatxCompletion),
}

pub type IoCompletionResult = Result<IoCompletion, IoOperationError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MultiCompletionResultState {
    Running,
    Draining
}

pub struct IoMultiCompletionResult {
    inner: VecDeque<IoCompletionResult>,
    op: IoOperation,
    state: MultiCompletionResultState,
}

impl IoMultiCompletionResult {

    pub fn new(result: IoCompletionResult, op: IoOperation) -> Self {
        let mut inner = VecDeque::new();
        inner.push_back(result);
        Self {
            inner,
            op,
            state: MultiCompletionResultState::Running
        }
    }

    pub fn complete(&mut self, result: i32) {
        let completion = self.op.complete(result);
        if let Err(e) = &completion {
            if let OsError::OperationCanceled = e.os_error {
                self.state = MultiCompletionResultState::Draining;
            }
        }
        self.inner.push_back(completion);
    }
}

pub enum IoCompletionState {
    NotCompleted(IoEntry),
    Single(IoCompletionResult),
    Multi(IoMultiCompletionResult),
    Completed,
}

impl IoCompletionState {
    pub fn new(entry: IoEntry) -> Self {
        Self::NotCompleted(entry)
    }

    pub fn complete(mut self, result: i32) -> Option<Self> {
        match self {
            Self::NotCompleted(entry) => {
                Some(entry.complete(result))
            },
            Self::Single(_) => None,
            Self::Multi(ref mut mcr) => {
                mcr.complete(result);
                Some(self)
            },
            Self::Completed => { None }
        }
    }

    pub fn into_result(self) -> (Option<IoCompletionResult>, Option<Self>) {
        match self {
            Self::NotCompleted(_) => { (None, Some(self)) },
            Self::Single(result) => { (Some(result), None) },
            Self::Multi(mut mcr) => {

                let is_empty = mcr.inner.len() <= 1;

                let res = 
                    if let Some(result) = mcr.inner.pop_front() {
                        Some(result)
                    } else { None }
                ;

                let r_self = 
                    if matches!(
                        mcr.state, 
                        MultiCompletionResultState::Draining
                    ) && is_empty { None }
                    else { Some(Self::Multi(mcr)) }
                ;
                        

                (res, r_self)
            },
            Self::Completed => { (None, None) }
        }
    }
}

pub trait TryFromCompletion: Sized {
    fn try_from_completion(completion: IoCompletion) -> Option<Self>;
}
