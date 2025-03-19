use std::{
    os::fd::{
        AsRawFd, 
        OwnedFd
    }, 
    pin::Pin, 
    sync::Arc, 
    task::{
        Context, 
        Poll
    }
};

use crate::{
    io::{
        fs::file::FileError, 
        IOCompletion, 
        IOOperation
    }, 
    worker::WorkerIOSubmissionHandle
};

use super::{FileFuture, FileMultiFuture, JoinableFileFuture, ToFileFuture};

pub struct FileWriteFuture {
    fd: Arc<OwnedFd>,
    offset: Option<usize>,
    data: Option<Vec<u8>>,
    handle: Option<WorkerIOSubmissionHandle>
}

impl FileWriteFuture {
    pub fn new(fd: Arc<OwnedFd>, offset: Option<usize>, data: Vec<u8>) -> Self {
        Self {
            fd,
            offset,
            data: Some(data),
            handle: None,
        }
    }

    pub fn as_io_operation(&mut self) -> Result<IOOperation, FileError> {
        let buffer = match self.data.take() {
            Some(data) => data,
            None => {
                return Err(FileError::InvalidWriteData)
            }
        };
        Ok(match self.offset {
            None => {
                IOOperation::write(
                    self.fd.as_raw_fd(),
                    buffer
                )
            },
            Some(offset) => {
                IOOperation::write_at(
                    self.fd.as_raw_fd(),
                    offset,
                    buffer
                )
            }
        })
    }
}

impl Future for FileWriteFuture {
    type Output = Result<usize, FileError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match &mut self.handle {
            None => {
                
                let op = self.as_io_operation()?;
                let handle = crate::submit_io_operation(
                    op,
                    Some(cx.waker().clone()),
                ).map_err(
                    |e| FileError::FailedToSubmitWriteOperationRequest {
                        source: e,
                    }
                )?;
                self.handle = Some(handle);
                Poll::Pending
            },
            Some(handle) => {
                if handle.is_completed() {
                    let handle = self.handle.take().unwrap();
                    if let Some(result) = handle.complete() {
                        if let Err(code) = result {
                            Poll::Ready(Err(FileError::IOUringFailure(code)))
                        } else {
                            let result = result.unwrap();
                            if let IOCompletion::Write(completion) = result {
                                Poll::Ready(Ok(completion.bytes_written))
                            } else {
                                Poll::Ready(Err(FileError::UnknownIOError))
                            }
                        }
                    } else {
                        Poll::Ready(Err(FileError::UnknownIOError))
                    }
                } else {
                    Poll::Pending
                }
            },
        }
    }
}

impl ToFileFuture for FileWriteFuture {
    fn to_file_future(self) -> FileFuture {
        FileFuture::Write(self)
    }
}

impl JoinableFileFuture for FileWriteFuture {
    fn join<F>(self, other: F) -> FileMultiFuture
    where
        Self: ToFileFuture + Sized,
        F: ToFileFuture + JoinableFileFuture
    {
        FileMultiFuture::new(vec![
            self.to_file_future(), 
            other.to_file_future()
        ])
    }
}