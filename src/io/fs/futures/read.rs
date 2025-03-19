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

use super::{
    FileFuture, FileMultiFuture, JoinableFileFuture, ToFileFuture
};


pub struct FileReadFuture {
    fd: Arc<OwnedFd>,
    offset: Option<usize>,
    len: usize,
    handle: Option<WorkerIOSubmissionHandle>
}

impl FileReadFuture {
    pub fn new(fd: Arc<OwnedFd>, offset: Option<usize>, len: usize) -> Self {
        Self { fd, offset, len, handle: None }
    }

    pub fn as_io_operation(&self) -> IOOperation {
        match self.offset {
            None => {
                IOOperation::read(
                    self.fd.as_raw_fd(),
                    self.len,
                )
            },
            Some(offset) => {
                IOOperation::read_at(
                    self.fd.as_raw_fd(),
                    offset,
                    self.len,
                )
            }
        }
    }

}

impl Future for FileReadFuture {
    type Output = Result<Vec<u8>, FileError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {

        match &mut self.handle {
            None => {
                let op = self.as_io_operation();
                self.handle = Some(crate::submit_io_operation(
                    op,
                    Some(cx.waker().clone())
                ).map_err(
                    |e| FileError::FailedToSubmitReadOperationRequest {
                        source: e,
                    }
                )?);
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
                            if let IOCompletion::Read(completion) = result {
                                Poll::Ready(Ok(completion.buffer))
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
            }
        }
    }
}

impl ToFileFuture for FileReadFuture {
    fn to_file_future(self) -> FileFuture {
        FileFuture::Read(self)
    }
}

impl JoinableFileFuture for FileReadFuture {
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