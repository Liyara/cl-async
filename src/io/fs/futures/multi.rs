use std::{
    pin::Pin, 
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
    worker::WorkerMultipleIOSubmissionHandle
};

use super::{FileFuture, JoinableFileFuture, ToFileFuture};

#[derive(Debug, Clone)]
pub enum FileOperationResult {
    Read(Vec<u8>),
    Write(usize),
}

enum FileMultiFutureState {
    NotSubmitted,
    Pending,
}

pub struct FileMultiFuture {
    children: Vec<FileFuture>,
    state: FileMultiFutureState,
    results: Option<Vec<Option<FileOperationResult>>>,
    handle: Option<WorkerMultipleIOSubmissionHandle>
}

impl FileMultiFuture {
    pub fn new(children: Vec<FileFuture>) -> Self {
        let len = children.len();
        Self {
            children: children,
            state: FileMultiFutureState::NotSubmitted,
            results: Some(vec![None; len]),
            handle: None,
        }
    }

    fn get_operations(&mut self) -> Result<(Vec<IOOperation>, Vec<FileFuture>), FileError> {
        let mut ops: Vec<IOOperation> = Vec::with_capacity(self.children.len());
        let mut r_children = Vec::with_capacity(self.children.len());

        for mut child in self.children.drain(..) {
            match child {
                FileFuture::Read(ref future) => {
                    ops.push(future.as_io_operation());
                    r_children.push(child);
                },
                FileFuture::Write(ref mut future) => {
                    ops.push(future.as_io_operation()?);
                    r_children.push(child);
                },
                FileFuture::Multi(ref mut future) => {
                    let (n_ops, n_children) = future.get_operations()?;
                    ops.extend(n_ops);
                    r_children.extend(n_children);
                }
            }
        }

        Ok((ops, r_children))
    }
}

impl Future for FileMultiFuture {
    type Output = Result<Vec<Option<FileOperationResult>>, FileError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {

        let this = self.as_mut().get_mut();

        match this.state {

            FileMultiFutureState::NotSubmitted => {

                let (ops, _) = this.get_operations()?;

                this.handle = Some(crate::submit_io_operations(
                    ops, 
                    Some(cx.waker().clone())
                ).map_err(
                    |e| FileError::FailedToSubmitIOOperationRequest {
                        source: e
                    }
                )?);

                this.state = FileMultiFutureState::Pending;
                Poll::Pending
            },

            FileMultiFutureState::Pending => {

                let handle = match this.handle.as_mut() {
                    Some(handle) => handle,
                    None => return Poll::Ready(Err(FileError::UnknownIOError))
                };

                let (completed_keys, is_completed) = (
                    handle.complete(),
                    handle.is_completed()
                );

                let results = match this.results.as_mut() {
                    Some(results) => results,
                    None => return Poll::Ready(Err(FileError::UnknownIOError))
                };

                for (result, index) in completed_keys {
                    if index >= results.len() {
                        log::error!("Index {} out of bounds ({})", index, results.len());
                        return Poll::Ready(Err(FileError::UnknownIOError));
                    }

                    results[index] = Some(match result {
                        Ok(result) => match result {
                            IOCompletion::Read(completion) => {
                                FileOperationResult::Read(completion.buffer)
                            },
                            IOCompletion::Write(completion) => {
                                FileOperationResult::Write(completion.bytes_written)
                            }
                        },
                        Err(e) => {
                            return Poll::Ready(Err(FileError::IOUringFailure(e)));
                        }
                    });
                }

                if is_completed {
                    Poll::Ready(Ok(this.results.take().unwrap()))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

impl ToFileFuture for FileMultiFuture {
    fn to_file_future(self) -> FileFuture {
        FileFuture::Multi(self)
    }
}

impl JoinableFileFuture for FileMultiFuture {
    fn join<F>(mut self, other: F) -> FileMultiFuture
    where
        Self: ToFileFuture + Sized,
        F: ToFileFuture + JoinableFileFuture
    {
        self.children.push(other.to_file_future());
        self.results.as_mut().unwrap().push(None);
        self
    }
}