use std::{os::fd::AsRawFd, task::Poll};

use crate::{io::{completion::TryFromCompletion, IoCompletion, IoError, IoOperation, IoOperationError, IoSubmissionError}, worker::WorkerIOSubmissionHandle};

pub struct IoOperationFuture<T: TryFromCompletion> {
    operation: Option<IoOperation>,
    handle: Option<WorkerIOSubmissionHandle>,
    _marker: std::marker::PhantomData<T>
}

impl<T: TryFromCompletion> IoOperationFuture<T> {
    pub fn new(operation: IoOperation) -> Self {
        Self {
            operation: Some(operation),
            handle: None,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: std::marker::Unpin + TryFromCompletion> Future for IoOperationFuture<T> {
    type Output = Result<T, IoError>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let this = self.as_mut().get_mut();
        
        match &mut this.handle {
            None => {

                this.handle = Some(crate::submit_io_operation(
                    this.operation.take().unwrap(),
                    Some(cx.waker().clone())
                ).map_err(|e| {
                    IoSubmissionError::FailedToSubmitIoOperationRequest(
                        e.to_string()
                    )
                })?);

                Poll::Pending
            },
            Some(handle) => {
                let poll_r = handle.poll(cx);

                match poll_r {
                    Poll::Ready(Some(Ok(completion))) => {
                        let result = T::try_from_completion(completion)?;
                        Poll::Ready(Ok(result))
                    },
                    Poll::Ready(Some(Err(e))) => {
                        Poll::Ready(Err(IoError::Operation(e)))
                    },
                    Poll::Pending => {
                        Poll::Pending
                    },
                    Poll::Ready(None) => {
                        Poll::Ready(Err(IoOperationError::UnexpectedPollResult(
                            String::from(
                                "Poll returned ready but no data was found"
                            )
                        ).into()))
                    }
                }
            }
        }
    }
}




/*
    Type definitions for easier integration with future.
*/

// Read Buffer

impl TryFromCompletion for Vec<u8> {
    
    fn try_from_completion(completion: IoCompletion) -> Result<Self, IoError> {
        match completion {
            IoCompletion::Read(completion) => {
                Ok(completion.data)
            }
            _ => {
                Err(IoOperationError::UnexpectedPollResult(
                    String::from("Io read future got unexpected completion type")
                ).into())
            }
        }
    }
}

pub type IoReadFuture = IoOperationFuture<Vec<u8>>;

impl TryFromCompletion for usize {
    
    fn try_from_completion(completion: IoCompletion) -> Result<Self, IoError> {
        match completion {
            IoCompletion::Write(data) => {
                Ok(data.bytes_written)
            }
            _ => {
                Err(IoOperationError::UnexpectedPollResult(
                    String::from("Io write future got unexpected completion type")
                ).into())
            }
        }
    }
}
pub type IoWriteFuture = IoOperationFuture<usize>;


impl TryFromCompletion for Vec<Vec<u8>> {
    
    fn try_from_completion(completion: IoCompletion) -> Result<Self, IoError> {
        match completion {
            IoCompletion::MultiRead(completion) => {
                Ok(completion.data)
            }
            _ => {
                Err(IoOperationError::UnexpectedPollResult(
                    String::from("Io multi read future got unexpected completion type")
                ).into())
            }
        }
    }
}

pub type IoMultiReadFuture = IoOperationFuture<Vec<Vec<u8>>>;


// Void futures are for operations that either succeed or fail, with no other data
pub type IoVoidFuture = IoOperationFuture<()>;

impl TryFromCompletion for () {
    fn try_from_completion(completion: IoCompletion) -> Result<Self, IoError> {
        match completion {
            IoCompletion::Success => Ok(()),
            _ => Err(IoOperationError::UnexpectedPollResult(
                String::from("Io void future got unexpected completion type")
            ).into())
        }
    }
}

/*

    Traits defining async operations on AsRawFd types.

*/

pub trait AsyncGatherReadable: AsRawFd {
    type Error;
    type OutputBuffer: Into<Vec<u8>>;
    type InputBuffer: From<Vec<u8>>;
    fn readv(&self, buffers_lengths: Vec<usize>) -> impl Future<Output = Result<Vec<Self::OutputBuffer>, Self::Error>>;
    fn readv_into(&self, buffers: Vec<Self::InputBuffer>) -> impl Future<Output = Result<Vec<Self::OutputBuffer>, Self::Error>>;
    fn readv_at(&self, offset: usize, buffers_lengths: Vec<usize>) -> impl Future<Output = Result<Vec<Self::OutputBuffer>, Self::Error>>;
    fn readv_at_into(&self, offset: usize, buffers: Vec<Self::InputBuffer>) -> impl Future<Output = Result<Vec<Self::OutputBuffer>, Self::Error>>;
}

pub trait AsyncReadable: AsRawFd + AsyncGatherReadable {
    fn read(&self, buffer_length: usize) -> impl Future<Output = Result<Self::OutputBuffer, Self::Error>>;
    fn read_into(&self, buffer: Self::InputBuffer) -> impl Future<Output = Result<Self::OutputBuffer, Self::Error>>;
    fn read_at(&self, offset: usize, buffer_length: usize) -> impl Future<Output = Result<Self::OutputBuffer, Self::Error>>;
    fn read_at_into(&self, offset: usize, buffer: Self::InputBuffer) -> impl Future<Output = Result<Self::OutputBuffer, Self::Error>>;
}

pub trait AsyncScatterWritable: AsRawFd {
    type Error;
    type InputBuffer: From<Vec<u8>>;
    type Output: Into<usize>;
    fn writev(&self, buffers: Vec<Self::InputBuffer>) -> impl Future<Output = Result<Self::Output, Self::Error>>;
    fn writev_at(&self, offset: usize, buffers: Vec<Self::InputBuffer>) -> impl Future<Output = Result<Self::Output, Self::Error>>;
}

pub trait AsyncWritable: AsRawFd + AsyncScatterWritable {
    fn write(&self, buffer: Self::InputBuffer) -> impl Future<Output = Result<Self::Output, Self::Error>>;
    fn write_at(&self, offset: usize, buffer: Self::InputBuffer) -> impl Future<Output = Result<Self::Output, Self::Error>>;
}

pub trait AsyncMessageReceiver: AsRawFd {

    type Error;
    type InputBuffer: From<Vec<u8>>;
    type OutputMessage: Into<crate::io::message::IoMessage>;

    fn recv_msg(
        &self,
        buffer_lengths: Vec<usize>,
        control_length: usize,
        flags: crate::io::operation::data::IoRecvMsgInputFlags
    ) -> impl Future<Output = Result<Self::OutputMessage, Self::Error>>;

    fn recv_msg_into(
        &self,
        buffers: Option<Vec<Self::InputBuffer>>,
        control: Option<Self::InputBuffer>,
        flags: crate::io::operation::data::IoRecvMsgInputFlags
    ) -> impl Future<Output = Result<Self::OutputMessage, Self::Error>>;
}

pub trait AsyncReceiver: AsRawFd + AsyncMessageReceiver {

    type OutputBuffer: Into<Vec<u8>>;
    
    fn recv(
        &self, 
        buffer_length: usize,
        flags: crate::io::operation::data::IoRecvFlags
    ) -> impl Future<Output = Result<Self::OutputBuffer, Self::Error>>;

    fn recv_into(
        &self, 
        buffer: Self::InputBuffer,
        flags: crate::io::operation::data::IoRecvFlags
    ) -> impl Future<Output = Result<Self::OutputBuffer, Self::Error>>;
}

pub trait AsyncMessageSender: AsRawFd {

    type Error;
    type InputMessage: Into<crate::io::message::IoMessage>;
    type Output: Into<usize>;

    fn send_msg(
        &self, 
        message: Self::InputMessage,
        flags: crate::io::operation::data::IoSendFlags
    ) -> impl Future<Output = Result<Self::Output, Self::Error>>;
}

pub trait AsyncSender: AsRawFd + AsyncMessageSender {

    type InputBuffer: From<Vec<u8>>;

    fn send(
        &self, 
        buffer: Self::InputBuffer,
        flags: crate::io::operation::data::IoSendFlags
    ) -> impl Future<Output = Result<Self::Output, Self::Error>>;
}

pub trait AsyncCopyable: AsRawFd {

    type Error;

    fn copy_to<T: AsRawFd>(
        &self,
        fd: &T,
        buffer_length: usize,
        offset_in: usize,
        offset_out: usize,
        flags: crate::io::operation::data::IoSpliceFlags
    ) -> impl Future<Output = Result<usize, Self::Error>>;
}

/* 

    Macros for typical implementation of async traits

*/

pub (crate) macro __async_impl_read__ {
    () => {
        async fn read(&self, buffer_length: usize) -> crate::io::IoResult<Vec<u8>> {
            Ok(crate::io::operation::future::IoReadFuture::new(
                crate::io::IoOperation::read(
                    self,
                    buffer_length,
                )
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_read_into__ {
    () => {
        async fn read_into(&self, buffer: Vec<u8>) -> crate::io::IoResult<Vec<u8>> {
            Ok(crate::io::operation::future::IoReadFuture::new(
                crate::io::IoOperation::read_into(
                    self,
                    crate::io::IoOutputBuffer::new(buffer)?
                )
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_read_at__ {
    () => {
        async fn read_at(&self, offset: usize, buffer_length: usize) -> crate::io::IoResult<Vec<u8>> {
            Ok(crate::io::operation::future::IoReadFuture::new(
                crate::io::IoOperation::read_at(
                    self,
                    offset,
                    buffer_length
                )
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_read_at_into__ {
    () => {
        async fn read_at_into(&self, offset: usize, buffer: Vec<u8>) -> crate::io::IoResult<Vec<u8>> {
            Ok(crate::io::operation::future::IoReadFuture::new(
                crate::io::IoOperation::read_at_into(
                    self,
                    offset,
                    crate::io::IoOutputBuffer::new(buffer)?
                )
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_readv__ {
    () => {
        async fn readv(&self, buffers_lengths: Vec<usize>) -> crate::io::IoResult<Vec<Vec<u8>>> {
            Ok(crate::io::operation::future::IoMultiReadFuture::new(
                crate::io::IoOperation::readv(
                    self,
                    buffers_lengths
                )?
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_readv_into__ {
    () => {
        async fn readv_into(&self, buffers: Vec<Vec<u8>>) -> crate::io::IoResult<Vec<Vec<u8>>> {
            Ok(crate::io::operation::future::IoMultiReadFuture::new(
                crate::io::IoOperation::readv_into(
                    self,
                    crate::io::IoDoubleOutputBuffer::new(buffers)?
                )?
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_readv_at__ {
    () => {
        async fn readv_at(&self, offset: usize, buffers_lengths: Vec<usize>) -> crate::io::IoResult<Vec<Vec<u8>>> {
            Ok(crate::io::operation::future::IoMultiReadFuture::new(
                crate::io::IoOperation::readv_at(
                    self,
                    offset,
                    buffers_lengths
                )?
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_readv_at_into__ {
    () => {
        async fn readv_at_into(&self, offset: usize, buffers: Vec<Vec<u8>>) -> crate::io::IoResult<Vec<Vec<u8>>> {
            Ok(crate::io::operation::future::IoMultiReadFuture::new(
                crate::io::IoOperation::readv_at_into(
                    self,
                    offset,
                    crate::io::IoDoubleOutputBuffer::new(buffers)?
                )?
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_gather_readable__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncGatherReadable for $type {
            type Error = crate::io::IoError;
            type OutputBuffer = Vec<u8>;
            type InputBuffer = Vec<u8>;
            crate::io::operation::future::__async_impl_readv__!();
            crate::io::operation::future::__async_impl_readv_into__!();
            crate::io::operation::future::__async_impl_readv_at__!();
            crate::io::operation::future::__async_impl_readv_at_into__!();
        }
    }
}

pub (crate) macro __async_impl_readable__ {
    ($type:ty) => {
        __async_impl_gather_readable__!($type);
        impl crate::io::operation::future::AsyncReadable for $type {
            crate::io::operation::future::__async_impl_read__!();
            crate::io::operation::future::__async_impl_read_into__!();
            crate::io::operation::future::__async_impl_read_at__!();
            crate::io::operation::future::__async_impl_read_at_into__!();
        }
    }
}

pub (crate) macro __async_impl_write__ {
    () => {
        async fn write(&self, buffer: Vec<u8>) -> crate::io::IoResult<usize> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::write(
                    self,
                    crate::io::IoInputBuffer::new(buffer)?
                )
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_write_at__ {
    () => {
        async fn write_at(&self, offset: usize, buffer: Vec<u8>) -> crate::io::IoResult<usize> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::write_at(
                    self,
                    offset,
                    crate::io::IoInputBuffer::new(buffer)?
                )
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_writev__ {
    () => {
        async fn writev(&self, buffers: Vec<Vec<u8>>) -> crate::io::IoResult<usize> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::writev(
                    self,
                    crate::io::IoDoubleInputBuffer::new(buffers)?
                )?
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_writev_at__ {
    () => {
        async fn writev_at(&self, offset: usize, buffers: Vec<Vec<u8>>) -> crate::io::IoResult<usize> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::writev_at(
                    self,
                    offset,
                    crate::io::IoDoubleInputBuffer::new(buffers)?
                )?
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_scatter_writable__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncScatterWritable for $type {
            type Error = crate::io::IoError;
            type InputBuffer = Vec<u8>;
            type Output = usize;
            crate::io::operation::future::__async_impl_writev__!();
            crate::io::operation::future::__async_impl_writev_at__!();
        }
    }
}

pub (crate) macro __async_impl_writable__ {
    ($type:ty) => {
        __async_impl_scatter_writable__!($type);
        impl crate::io::operation::future::AsyncWritable for $type {
            crate::io::operation::future::__async_impl_write__!();
            crate::io::operation::future::__async_impl_write_at__!();
        }
    }
}

pub (crate) macro __async_impl_copy_to__ {
    () => {
        async fn copy_to<T: AsRawFd>(
            &self,
            fd: &T,
            buffer_length: usize,
            offset_in: usize,
            offset_out: usize,
            flags: crate::io::operation::data::IoSpliceFlags,
        ) -> crate::io::IoResult<usize> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::splice(
                    self,
                    fd,
                    buffer_length,
                    offset_in,
                    offset_out,
                    flags
                )
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_copyable__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncCopyable for $type {
            type Error = crate::io::IoError;
            crate::io::operation::future::__async_impl_copy_to__!();
        }
    }
}

pub (crate) macro __async_impl_recv__ {
    () => {
        async fn recv(
            &self, 
            buffer_length: usize,
            flags: crate::io::operation::data::IoRecvFlags
        ) -> crate::io::IoResult<Vec<u8>> {
            Ok(crate::io::operation::future::IoReadFuture::new(
                crate::io::IoOperation::recv(
                    self,
                    buffer_length,
                    flags
                )
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_recv_into__ {
    () => {
        async fn recv_into(
            &self, 
            buffer: Vec<u8>,
            flags: crate::io::operation::data::IoRecvFlags
        ) -> crate::io::IoResult<Vec<u8>> {
            Ok(crate::io::operation::future::IoReadFuture::new(
                crate::io::IoOperation::recv_into(
                    self,
                    crate::io::IoOutputBuffer::new(buffer)?,
                    flags
                )
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_recv_msg__ {
    () => {
        async fn recv_msg(
            &self,
            buffer_lengths: Vec<usize>,
            control_length: usize,
            flags: crate::io::operation::data::IoRecvMsgInputFlags
        ) -> crate::io::IoResult<crate::io::message::IoMessage> {
            Ok(crate::io::message::IoMessageFuture::new(
                crate::io::IoOperation::recv_msg(
                    self,
                    buffer_lengths,
                    control_length,
                    flags
                )?
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_recv_msg_into__ {
    () => {
        async fn recv_msg_into(
            &self,
            buffers: Option<Vec<Vec<u8>>>,
            control: Option<Vec<u8>>,
            flags: crate::io::operation::data::IoRecvMsgInputFlags
        ) -> crate::io::IoResult<crate::io::message::IoMessage> {
            Ok(crate::io::message::IoMessageFuture::new(
                crate::io::IoOperation::recv_msg_into(
                    self,
                    buffers,
                    control,
                    flags
                )?
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_message_receiver__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncMessageReceiver for $type {
            type Error = crate::io::IoError;
            type InputBuffer = Vec<u8>;
            type OutputMessage = crate::io::message::IoMessage;
            crate::io::operation::future::__async_impl_recv_msg__!();
            crate::io::operation::future::__async_impl_recv_msg_into__!();
        }
    }
}

pub (crate) macro __async_impl_receiver__ {
    ($type:ty) => {
        __async_impl_message_receiver__!($type);
        impl crate::io::operation::future::AsyncReceiver for $type {
            type OutputBuffer = Vec<u8>;
            crate::io::operation::future::__async_impl_recv__!();
            crate::io::operation::future::__async_impl_recv_into__!();
        }
    }
}

pub (crate) macro __async_impl_send__ {
    () => {
        async fn send(
            &self, 
            buffer: Vec<u8>,
            flags: crate::io::operation::data::IoSendFlags
        ) -> crate::io::IoResult<usize> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::send(
                    self,
                    crate::io::IoInputBuffer::new(buffer)?,
                    flags
                )
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_send_msg__ {
    () => {
        async fn send_msg(
            &self, 
            message: crate::io::message::IoMessage,
            flags: crate::io::operation::data::IoSendFlags
        ) -> crate::io::IoResult<usize> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::send_msg(
                    self,
                    message,
                    flags
                )?
            ).await?.into())
        }
    }
}

pub (crate) macro __async_impl_message_sender__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncMessageSender for $type {
            type Error = crate::io::IoError;
            type InputMessage = crate::io::message::IoMessage;
            type Output = usize;
            crate::io::operation::future::__async_impl_send_msg__!();
        }
    }
}

pub (crate) macro __async_impl_sender__ {
    ($type:ty) => {
        __async_impl_message_sender__!($type);
        impl crate::io::operation::future::AsyncSender for $type {
            type InputBuffer = Vec<u8>;
            crate::io::operation::future::__async_impl_send__!();
        }
    }
}