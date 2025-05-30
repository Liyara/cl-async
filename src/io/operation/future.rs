use std::{os::fd::AsRawFd, task::Poll};

use bytes::{Bytes, BytesMut};

use crate::{io::{completion::TryFromCompletion, message::IoRecvMessage, IoCompletion, IoError, IoOperation, IoProcessingError, IoSubmissionError}, worker::WorkerIoSubmissionHandle};

pub struct IoOperationFuture<T: TryFromCompletion> {
    operation: Option<IoOperation>,
    handle: Option<WorkerIoSubmissionHandle>,
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
                    IoError::from(IoSubmissionError::from(e))
                })?);

                Poll::Pending
            },
            Some(handle) => {
                match handle.poll(cx) {
                    Poll::Ready(Some(Ok(completion))) => {
                        let result = T::try_from_completion(completion).ok_or(
                            IoProcessingError::UnexpectedCompletionResult(
                                String::from("Io operation future got unexpected completion type")
                            )
                        )?;
                        Poll::Ready(Ok(result))
                    },
                    Poll::Ready(Some(Err(e))) => {
                        Poll::Ready(Err(IoError::from(e)))
                    },
                    Poll::Pending => {
                        Poll::Pending
                    },
                    Poll::Ready(None) => {
                        Poll::Ready(Err(IoProcessingError::UnexpectedCompletionResult(
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

impl<T: TryFromCompletion> Drop for IoOperationFuture<T> {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            if let Err(e) = handle.cancel() {
                warn!("cl-async: IoOperationFuture: Failed to cancel operation: {e}");
            }
        }
    }
}


/*
    Type definitions for easier integration with future.
*/

// Read Buffer

impl TryFromCompletion for BytesMut {
    
    fn try_from_completion(completion: IoCompletion) -> Option<Self> {
        match completion {
            IoCompletion::Read(completion) => {
                Some(completion.buffer)
            }
            _ => None
        }
    }
}

impl TryFromCompletion for Vec<BytesMut> {
    
    fn try_from_completion(completion: IoCompletion) -> Option<Self> {
        match completion {
            IoCompletion::MultiRead(completion) => {
                Some(completion.buffers)
            }
            _ => None
        }
    }
}

impl TryFromCompletion for Bytes {
    
    fn try_from_completion(completion: IoCompletion) -> Option<Self> {
        BytesMut::try_from_completion(completion).map(|b| {
            b.freeze()
        })
    }
}

impl TryFromCompletion for Vec<Bytes> {
    
    fn try_from_completion(completion: IoCompletion) -> Option<Self> {
        Vec::<BytesMut>::try_from_completion(completion).map(|b| {
            b.into_iter().map(|b| b.freeze()).collect()
        })
    }
}

impl TryFromCompletion for Vec<u8> {
    
    fn try_from_completion(completion: IoCompletion) -> Option<Self> {
        Bytes::try_from_completion(completion).map(|b| {
            b.to_vec()
        })
    }
}

impl TryFromCompletion for Vec<Vec<u8>> {
    
    fn try_from_completion(completion: IoCompletion) -> Option<Self> {
        Vec::<Bytes>::try_from_completion(completion).map(|b| {
            b.into_iter().map(|b| b.to_vec()).collect()
        })
    }
}

pub struct IoReadOutput<T: TryFromCompletion> {
    pub buffer: T,
    pub bytes_read: usize,
}

impl<T: TryFromCompletion> IoReadOutput<T> {
    pub fn into_buffer(self) -> T {
        self.buffer
    }
}

impl<T: TryFromCompletion> TryFromCompletion for IoReadOutput<T> {
    
    fn try_from_completion(completion: IoCompletion) -> Option<Self> {

        let bytes_read = match &completion {
            IoCompletion::Read(completion_inner) => {
                completion_inner.bytes_read
            },
            IoCompletion::MultiRead(completion_inner) => {
                completion_inner.bytes_read
            }
            _ => return None
        };

        let buffer = T::try_from_completion(completion)?;

        Some(IoReadOutput {
            buffer,
            bytes_read
        })
    }
}
            

pub type IoReadFuture = IoOperationFuture<Bytes>;
pub type IoReadIntoFuture = IoOperationFuture<IoReadOutput<BytesMut>>;
pub type IoMultiReadFuture = IoOperationFuture<IoReadOutput<Vec<Bytes>>>;
pub type IoMultiReadIntoFuture = IoOperationFuture<IoReadOutput<Vec<BytesMut>>>;

impl TryFromCompletion for usize {
    
    fn try_from_completion(completion: IoCompletion) -> Option<Self> {
        match completion {
            IoCompletion::Write(data) => {
                Some(data.bytes_written)
            }
            _ => None
        }
    }
}
pub type IoWriteFuture = IoOperationFuture<usize>;

impl TryFromCompletion for IoRecvMessage {
    
    fn try_from_completion(completion: IoCompletion) -> Option<Self> {
        match completion {
            IoCompletion::Msg(data) => {
                Some(data.msg)
            }
            _ => None
        }
    }
}

pub type IoMessageFuture = IoOperationFuture<IoRecvMessage>;
                
// Void futures are for operations that either succeed or fail, with no other data
pub type IoVoidFuture = IoOperationFuture<()>;

impl TryFromCompletion for () {
    fn try_from_completion(completion: IoCompletion) -> Option<Self> {
        match completion {
            IoCompletion::Success => Some(()),
            _ => None
        }
    }
}

/*

    Traits defining async operations on AsRawFd types.

*/

pub trait AsyncIoTypes: AsRawFd {
    type Error;
}

pub trait AsyncIoWriteMessageTypes: AsyncIoTypes {
    type InputMessage;
}

pub trait AsyncIoGatherReadTypes: AsyncIoTypes {
    type ReadvIntoOutputBuffers;
    type ReadvInputBuffers;
    type ReadvOutputBuffers;
}

pub trait AsyncIoReadTypes: AsyncIoTypes {
    type ReadOutputBuffer;
    type ReadIntoOutputBuffer;
    type ReadInputBuffer;
}

pub trait AsyncIoWriteTypes: AsyncIoTypes {
    type OutputBytesWritten;
    type WriteInputBuffer;
}

pub trait AsyncIoScatterWriteTypes: AsyncIoTypes {
    type WritevInputBuffer;
}

pub trait AsyncIoReadMessageTypes: AsyncIoTypes {
    type OutputMessage;
    type ReadMessageInputBuffer;
}

pub trait AsyncGatherReadable: AsyncIoGatherReadTypes {
    fn readv(&self, buffers_lengths: Vec<usize>) -> impl Future<Output = Result<Self::ReadvOutputBuffers, Self::Error>>;
    fn readv_into(&self, buffers: Self::ReadvInputBuffers) -> impl Future<Output = Result<Self::ReadvIntoOutputBuffers, Self::Error>>;
    fn readv_at(&self, offset: usize, buffers_lengths: Vec<usize>) -> impl Future<Output = Result<Self::ReadvOutputBuffers, Self::Error>>;
    fn readv_at_into(&self, offset: usize, buffers: Self::ReadvInputBuffers) -> impl Future<Output = Result<Self::ReadvIntoOutputBuffers, Self::Error>>;
}

pub trait AsyncReadable: AsyncIoReadTypes + AsyncGatherReadable {
    fn read(&self, buffer_length: usize) -> impl Future<Output = Result<Self::ReadOutputBuffer, Self::Error>>;
    fn read_into(&self, buffer: Self::ReadInputBuffer) -> impl Future<Output = Result<Self::ReadIntoOutputBuffer, Self::Error>>;
    fn read_at(&self, offset: usize, buffer_length: usize) -> impl Future<Output = Result<Self::ReadOutputBuffer, Self::Error>>;
    fn read_at_into(&self, offset: usize, buffer: Self::ReadInputBuffer) -> impl Future<Output = Result<Self::ReadIntoOutputBuffer, Self::Error>>;
}

pub trait AsyncScatterWritable: AsyncIoWriteTypes + AsyncIoScatterWriteTypes {
    fn writev(&self, buffers: Self::WritevInputBuffer) -> impl Future<Output = Result<Self::OutputBytesWritten, Self::Error>>;
    fn writev_at(&self, offset: usize, buffers: Self::WritevInputBuffer) -> impl Future<Output = Result<Self::OutputBytesWritten, Self::Error>>;
}

pub trait AsyncWritable: AsRawFd + AsyncScatterWritable {
    fn write(&self, buffer: Self::WriteInputBuffer) -> impl Future<Output = Result<Self::OutputBytesWritten, Self::Error>>;
    fn write_at(&self, offset: usize, buffer: Self::WriteInputBuffer) -> impl Future<Output = Result<Self::OutputBytesWritten, Self::Error>>;
}

pub trait AsyncMessageReceiver: AsyncIoReadMessageTypes {

    fn recv_msg(
        &self,
        buffer_lengths: Vec<usize>,
        control_length: usize,
        flags: crate::io::operation::data::IoRecvMsgInputFlags
    ) -> impl Future<Output = Result<Self::OutputMessage, Self::Error>>;

    fn recv_msg_into(
        &self,
        buffers: Option<Vec<Self::ReadMessageInputBuffer>>,
        control: Option<Self::ReadMessageInputBuffer>,
        flags: crate::io::operation::data::IoRecvMsgInputFlags
    ) -> impl Future<Output = Result<Self::OutputMessage, Self::Error>>;
}

pub trait AsyncReceiver: AsyncMessageReceiver + AsyncIoReadTypes {
    
    fn recv(
        &self, 
        buffer_length: usize,
        flags: crate::io::operation::data::IoRecvFlags
    ) -> impl Future<Output = Result<Self::ReadOutputBuffer, Self::Error>>;

    fn recv_into(
        &self, 
        buffer: Self::ReadInputBuffer,
        flags: crate::io::operation::data::IoRecvFlags
    ) -> impl Future<Output = Result<Self::ReadIntoOutputBuffer, Self::Error>>;
}

pub trait AsyncMessageSender: AsyncIoWriteMessageTypes + AsyncIoWriteTypes {

    fn send_msg(
        &self, 
        message: Self::InputMessage,
        flags: crate::io::operation::data::IoSendFlags
    ) -> impl Future<Output = Result<Self::OutputBytesWritten, Self::Error>>;
}

pub trait AsyncSender: AsyncMessageSender {

    fn send(
        &self, 
        buffer: Self::WriteInputBuffer,
        flags: crate::io::operation::data::IoSendFlags
    ) -> impl Future<Output = Result<Self::OutputBytesWritten, Self::Error>>;
}

pub trait AsyncCopyable: AsyncIoWriteTypes {

    fn copy_to<T: AsRawFd>(
        &self,
        fd: &T,
        buffer_length: usize,
        offset_in: usize,
        offset_out: usize,
        flags: crate::io::operation::data::IoSpliceFlags
    ) -> impl Future<Output = Result<Self::OutputBytesWritten, Self::Error>>;
}

/* 

    Macros for typical implementations of async traits
    These are for internal use in this crate only and represent
    low-level types abstracted from OS IO operations.

*/

pub (crate) macro __async_impl_types__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncIoTypes for $type {
            type Error = crate::io::IoError;
        }
        impl crate::io::operation::future::AsyncIoGatherReadTypes for $type {
            type ReadvInputBuffers = Vec<BytesMut>;
            type ReadvIntoOutputBuffers = IoReadOutput<Vec<BytesMut>>;
            type ReadvOutputBuffers = Vec<Bytes>;
        }
        impl crate::io::operation::future::AsyncIoReadTypes for $type {
            type ReadOutputBuffer = Bytes;
            type ReadIntoOutputBuffer = IoReadOutput<BytesMut>;
            type ReadInputBuffer = BytesMut;
        }
        impl crate::io::operation::future::AsyncIoWriteTypes for $type {
            type OutputBytesWritten = usize;
            type WriteInputBuffer = Bytes;
        }
        impl crate::io::operation::future::AsyncIoScatterWriteTypes for $type {
            type WritevInputBuffer = std::sync::Arc<Vec<Bytes>>;
        }
        impl crate::io::operation::future::AsyncIoReadMessageTypes for $type {
            type OutputMessage = crate::io::message::IoRecvMessage;
            type ReadMessageInputBuffer = BytesMut;
        }
        impl crate::io::operation::future::AsyncIoWriteMessageTypes for $type {
            type InputMessage = crate::io::message::IoSendMessage;
        }
    }
}

pub (crate) macro __async_impl_read__ {
    () => {
        async fn read(&self, buffer_length: usize) -> Result<Bytes, crate::io::IoError> {
            Ok(crate::io::operation::future::IoReadFuture::new(
                crate::io::IoOperation::read(
                    self,
                    buffer_length,
                )
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_read_into__ {
    () => {
        async fn read_into(&self, buffer: BytesMut) -> Result<IoReadOutput<BytesMut>, crate::io::IoError> {
            Ok(crate::io::operation::future::IoReadIntoFuture::new(
                crate::io::IoOperation::read_into(
                    self,
                    crate::io::IoOutputBuffer::new(buffer)
                        .map_err(crate::io::OutputBufferSubmissionError::from)
                        .map_err(crate::io::IoSubmissionError::from)
                    ?
                )
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_read_at__ {
    () => {
        async fn read_at(&self, offset: usize, buffer_length: usize) -> Result<Bytes, crate::io::IoError> {
            Ok(crate::io::operation::future::IoReadFuture::new(
                crate::io::IoOperation::read_at(
                    self,
                    offset,
                    buffer_length
                )
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_read_at_into__ {
    () => {
        async fn read_at_into(&self, offset: usize, buffer: BytesMut) -> Result<IoReadOutput<BytesMut>, crate::io::IoError> {
            Ok(crate::io::operation::future::IoReadIntoFuture::new(
                crate::io::IoOperation::read_at_into(
                    self,
                    offset,
                    crate::io::IoOutputBuffer::new(buffer)
                        .map_err(crate::io::OutputBufferSubmissionError::from)
                        .map_err(crate::io::IoSubmissionError::from)
                    ?,
                )
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_readv__ {
    () => {
        async fn readv(&self, buffers_lengths: Vec<usize>) -> Result<Vec<Bytes>, crate::io::IoError> {
            Ok(crate::io::operation::future::IoMultiReadFuture::new(
                crate::io::IoOperation::readv(
                    self,
                    buffers_lengths
                )?
            ).await?.buffer)
        }
    }
}

pub (crate) macro __async_impl_readv_into__ {
    () => {
        async fn readv_into(&self, buffers: Vec<BytesMut>) -> Result<IoReadOutput<Vec<BytesMut>>, crate::io::IoError> {
            Ok(crate::io::operation::future::IoMultiReadIntoFuture::new(
                crate::io::IoOperation::readv_into(
                    self,
                    crate::io::buffers::IoVecOutputBuffer::new(buffers)
                        .map_err(crate::io::OutputBufferVecSubmissionError::from)
                        .map_err(crate::io::IoSubmissionError::from)
                    ?
                )?
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_readv_at__ {
    () => {
        async fn readv_at(&self, offset: usize, buffers_lengths: Vec<usize>) -> Result<Vec<Bytes>, crate::io::IoError> {
            Ok(crate::io::operation::future::IoMultiReadFuture::new(
                crate::io::IoOperation::readv_at(
                    self,
                    offset,
                    buffers_lengths
                )?
            ).await?.buffer)
        }
    }
}

pub (crate) macro __async_impl_readv_at_into__ {
    () => {
        async fn readv_at_into(&self, offset: usize, buffers: Vec<BytesMut>) -> Result<IoReadOutput<Vec<BytesMut>>, crate::io::IoError> {
            Ok(crate::io::operation::future::IoMultiReadIntoFuture::new(
                crate::io::IoOperation::readv_at_into(
                    self,
                    offset,
                    crate::io::buffers::IoVecOutputBuffer::new(buffers)
                        .map_err(crate::io::OutputBufferVecSubmissionError::from)
                        .map_err(crate::io::IoSubmissionError::from)
                    ?
                )?
            ).await?)
        }
    }
}


pub (crate) macro __async_impl_gather_readable__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncGatherReadable for $type {
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
        async fn write(&self, buffer: Bytes) -> Result<usize, IoError> {

            let input_buffer_ret = crate::io::IoInputBuffer::new(buffer);

            match input_buffer_ret {
                Ok(input_buffer) => {
                    Ok(crate::io::operation::future::IoWriteFuture::new(
                        crate::io::IoOperation::write(
                            self,
                            input_buffer
                        )
                    ).await?)
                }
                Err(e) => {
                    match e.kind {
                        crate::io::InvalidIoInputBufferErrorKind::NonContiguousInputBuffer => {

                            let input_buffer = crate::io::buffers::IoVecInputBuffer::new_single(
                                e.buffer
                            ).map_err(
                                crate::io::InputBufferSubmissionError::from
                            ).map_err(
                                crate::io::IoSubmissionError::from
                            )?;

                            Ok(crate::io::operation::future::IoWriteFuture::new(
                                crate::io::IoOperation::writev(
                                    self,
                                    input_buffer
                                )?
                            ).await?)
                        }
                        _ => {
                            return Err(crate::io::IoError::from(
                                crate::io::IoSubmissionError::from(
                                    crate::io::InputBufferSubmissionError::from(e)
                                )
                            ));
                        }
                    }
                }
            }
        }
    }
}

pub (crate) macro __async_impl_writev__ {
    () => {
        async fn writev(&self, buffers: std::sync::Arc<Vec<Bytes>>) -> Result<usize, crate::io::IoError> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::writev(
                    self,
                    crate::io::buffers::IoVecInputBuffer::new_multiple(buffers)
                        .map_err(crate::io::InputBufferVecSubmissionError::from)
                        .map_err(crate::io::IoSubmissionError::from)
                    ?
                )?
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_write_at__ {
    () => {
        async fn write_at(&self, offset: usize, buffer: Bytes) -> Result<usize, crate::io::IoError> {

            let input_buffer_ret = crate::io::IoInputBuffer::new(buffer);

            match input_buffer_ret {
                Ok(input_buffer) => {
                    Ok(crate::io::operation::future::IoWriteFuture::new(
                        crate::io::IoOperation::write_at(
                            self,
                            offset,
                            input_buffer
                        )
                    ).await?)
                }
                Err(e) => {
                    match e.kind {
                        crate::io::InvalidIoInputBufferErrorKind::NonContiguousInputBuffer => {

                            let input_buffer = crate::io::buffers::IoVecInputBuffer::new_single(
                                e.buffer
                            ).map_err(
                                crate::io::InputBufferSubmissionError::from
                            ).map_err(
                                crate::io::IoSubmissionError::from
                            )?;

                            Ok(crate::io::operation::future::IoWriteFuture::new(
                                crate::io::IoOperation::writev_at(
                                    self,
                                    offset,
                                    input_buffer
                                )?
                            ).await?)
                        }
                        _ => {
                            return Err(crate::io::IoError::from(
                                crate::io::IoSubmissionError::from(
                                    crate::io::InputBufferSubmissionError::from(e)
                                )
                            ));
                        }
                    }
                }
            }
        }
    }
}

pub (crate) macro __async_impl_writev_at__ {
    () => {
        async fn writev_at(&self, offset: usize, buffers: std::sync::Arc<Vec<Bytes>>) -> Result<usize, crate::io::IoError> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::writev_at(
                    self,
                    offset,
                    crate::io::buffers::IoVecInputBuffer::new_multiple(buffers)
                        .map_err(crate::io::InputBufferVecSubmissionError::from)
                        .map_err(crate::io::IoSubmissionError::from)
                    ?
                )?
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_scatter_writable__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncScatterWritable for $type {
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
            flags: crate::io::operation::data::IoSpliceFlags
        ) -> Result<usize, crate::io::IoError> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::splice(
                    self,
                    fd,
                    buffer_length,
                    offset_in,
                    offset_out,
                    flags
                )
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_copyable__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncCopyable for $type {
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
        ) -> Result<Bytes, IoError> {
            Ok(crate::io::operation::future::IoReadFuture::new(
                crate::io::IoOperation::recv(
                    self,
                    buffer_length,
                    flags
                )
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_recv_into__ {
    () => {
        async fn recv_into(
            &self, 
            buffer: BytesMut,
            flags: crate::io::operation::data::IoRecvFlags
        ) -> Result<IoReadOutput<BytesMut>, crate::io::IoError> {
            Ok(crate::io::operation::future::IoReadIntoFuture::new(
                crate::io::IoOperation::recv_into(
                    self,
                    crate::io::IoOutputBuffer::new(buffer)
                        .map_err(crate::io::OutputBufferSubmissionError::from)
                        .map_err(crate::io::IoSubmissionError::from)
                    ?,
                    flags
                )
            ).await?)
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
        ) -> Result<crate::io::message::IoRecvMessage, crate::io::IoError> {
            Ok(crate::io::operation::future::IoMessageFuture::new(
                crate::io::IoOperation::recv_msg(
                    self,
                    buffer_lengths,
                    control_length,
                    flags
                )?
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_recv_msg_into__ {
    () => {
        async fn recv_msg_into(
            &self,
            buffers: Option<Vec<BytesMut>>,
            control: Option<BytesMut>,
            flags: crate::io::operation::data::IoRecvMsgInputFlags
        ) -> Result<crate::io::message::IoRecvMessage, crate::io::IoError> {

            let prepared_buffers = crate::io::RecvMsgBuffers::new(
                buffers,
                control
            ).prepare().map_err(IoSubmissionError::from)?;

            Ok(crate::io::operation::future::IoMessageFuture::new(
                crate::io::IoOperation::recv_msg_into(
                    self,
                    prepared_buffers,
                    flags
                )?
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_message_receiver__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncMessageReceiver for $type {
            crate::io::operation::future::__async_impl_recv_msg__!();
            crate::io::operation::future::__async_impl_recv_msg_into__!();
        }
    }
}

pub (crate) macro __async_impl_receiver__ {
    ($type:ty) => {
        __async_impl_message_receiver__!($type);
        impl crate::io::operation::future::AsyncReceiver for $type {
            crate::io::operation::future::__async_impl_recv__!();
            crate::io::operation::future::__async_impl_recv_into__!();
        }
    }
}

pub (crate) macro __async_impl_send__ {
    () => {
        async fn send(
            &self, 
            buffer: Bytes,
            flags: crate::io::operation::data::IoSendFlags
        ) -> Result<usize, crate::io::IoError> {

            let input_buffer_ret = crate::io::IoInputBuffer::new(buffer);
            match input_buffer_ret {
                Ok(input_buffer) => {
                    Ok(crate::io::operation::future::IoWriteFuture::new(
                        crate::io::IoOperation::send(
                            self,
                            input_buffer,
                            flags
                        )
                    ).await?)
                }
                Err(e) => {
                    match e.kind {
                        crate::io::InvalidIoInputBufferErrorKind::NonContiguousInputBuffer => {

                            let msg = crate::io::message::IoSendMessage::new(
                                Some(crate::io::IoSendMessageDataBufferType::Bytes(e.buffer)),
                                None,
                            );

                            Ok(crate::io::operation::future::IoWriteFuture::new(
                                crate::io::IoOperation::send_msg(
                                    self,
                                    msg,
                                    flags
                                )?
                            ).await?)
                        }
                        _ => {
                            return Err(crate::io::IoError::from(
                                crate::io::IoSubmissionError::from(
                                    crate::io::InputBufferSubmissionError::from(e)
                                )
                            ));
                        }
                    }
                }
            }
        }
    }
}

pub (crate) macro __async_impl_send_msg__ {
    () => {
        async fn send_msg(
            &self, 
            message: crate::io::message::IoSendMessage,
            flags: crate::io::operation::data::IoSendFlags
        ) -> Result<usize, crate::io::IoError> {
            Ok(crate::io::operation::future::IoWriteFuture::new(
                crate::io::IoOperation::send_msg(
                    self,
                    message,
                    flags
                )?
            ).await?)
        }
    }
}

pub (crate) macro __async_impl_message_sender__ {
    ($type:ty) => {
        impl crate::io::operation::future::AsyncMessageSender for $type {
            crate::io::operation::future::__async_impl_send_msg__!();
        }
    }
}


pub (crate) macro __async_impl_sender__ {
    ($type:ty) => {
        __async_impl_message_sender__!($type);
        impl crate::io::operation::future::AsyncSender for $type {
            crate::io::operation::future::__async_impl_send__!();
        }
    }
}