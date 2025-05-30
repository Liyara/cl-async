use std::sync::Arc;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use once_cell::sync::Lazy;
use thiserror::Error;

const DEFAULT_IOV_MAX: usize = 1024;

static IOV_MAX_LIMIT: Lazy<usize> = Lazy::new(|| {
    unsafe { *libc::__errno_location() = 0; }

    let limit = unsafe { libc::sysconf(libc::_SC_IOV_MAX) };

    if limit < 0 {
        let errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        if errno != 0 {
            warn!(
                "Failed to query _SC_IOV_MAX via sysconf (errno {}), using default limit {}.",
                errno, DEFAULT_IOV_MAX
            );
            DEFAULT_IOV_MAX
        } else {
            warn!(
                "_SC_IOV_MAX is indeterminate via sysconf, using default limit {}.",
                DEFAULT_IOV_MAX
            );
            DEFAULT_IOV_MAX
        }
    } else if limit == 0 {
        warn!("_SC_IOV_MAX is 0, using default limit {}.", DEFAULT_IOV_MAX);
        DEFAULT_IOV_MAX
    } else { limit as usize }

});

#[derive(Debug, Error)]
pub enum InvalidIoVecError {
    #[error("The generated iovec set is empty (no memory to read into / no data to write from).")]
    NoDataForIoVecs,

    #[error("The generated iovec is too large: requested {requested} iovecs, but the maximum is {max}.")]
    TooManyIoVecs {
        requested: usize,
        max: usize,
    },
}

fn assert_iovecs_valid(iovecs: &[libc::iovec]) -> Result<(), InvalidIoVecError> {
    if iovecs.is_empty() {
        Err(InvalidIoVecError::NoDataForIoVecs)
    } else if iovecs.len() > *IOV_MAX_LIMIT {
        Err(InvalidIoVecError::TooManyIoVecs {
            requested: iovecs.len(),
            max: *IOV_MAX_LIMIT,
        })
    } else { Ok(()) }
}

#[derive(Debug, Error)]
pub enum InvalidIoInputBufferErrorKind {
    #[error("The input buffer is empty.")]
    EmptyInputBuffer,

    #[error("The input buffer is non-contiguous. Use IoVecInputBuffer instead.")]
    NonContiguousInputBuffer,
}

#[derive(Debug, Error)]
#[error("Invalid input buffer: {kind}")]
pub struct InvalidIoInputBufferError {

    pub buffer: Bytes,

    #[source]
    pub kind: InvalidIoInputBufferErrorKind,
}

impl InvalidIoInputBufferError {
    pub fn empty_buffer(buffer: Bytes) -> Self {
        Self {
            buffer,
            kind: InvalidIoInputBufferErrorKind::EmptyInputBuffer,
        }
    }

    pub fn non_contiguous_buffer(buffer: Bytes) -> Self {
        Self {
            buffer,
            kind: InvalidIoInputBufferErrorKind::NonContiguousInputBuffer,
        }
    }
}

// INPUT BUFFER
#[derive(Debug)]
// Represents a contiguous, immutable buffer for writing from.
pub struct IoInputBuffer(Bytes);

impl IoInputBuffer {
    pub fn new(data: Bytes) -> Result<Self, InvalidIoInputBufferError> {

        // Writing empty data is incoherent
        if data.is_empty() { 
            return Err(InvalidIoInputBufferError::empty_buffer(data));
        }

        /* 
            MUST be a contiguous buffer

            For non-contiguous writes, look at IoVecInputBuffer / writev
        
        */
        if data.chunk().len() != data.len() {
            return Err(InvalidIoInputBufferError::non_contiguous_buffer(data));
        }

        Ok(Self(data))
    }

    pub fn as_bytes(&self) -> &Bytes {
        &self.0
    }

    pub fn into_bytes(self) -> Bytes { self.0 }

    pub unsafe fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }

    pub fn readable_len(&self) -> usize {
        self.0.len()
    }
}
// OUTPUT BUFFER

#[derive(Debug, Error)]
pub enum IoOutputBufferIntoBytesError {

    #[error("Buffer overflow: requested {requested} bytes, but only {available} bytes available.")]
    BufferOverflow {
        requested: usize,
        available: usize,
        buffer: BytesMut,
    }
}

impl IoOutputBufferIntoBytesError {
    pub fn as_buffer(&self) -> &BytesMut {
        match self {
            IoOutputBufferIntoBytesError::BufferOverflow { buffer, .. } => buffer,
        }
    }

    pub fn into_buffer(self) -> BytesMut {
        match self {
            IoOutputBufferIntoBytesError::BufferOverflow { buffer, .. } => buffer,
        }
    }
}

#[derive(Debug, Error)]
#[error("The output buffer is empty.")]
pub struct EmptyIoOutputBufferError {
    pub buffer: BytesMut,
}

#[derive(Debug)]
// Represents a contiguous, mutable buffer for reading into.
pub struct IoOutputBuffer(BytesMut);

impl IoOutputBuffer {

    pub fn with_capacity(capacity: usize) -> Self {
        Self(BytesMut::with_capacity(capacity))
    }
    
    pub fn new(data: BytesMut) -> Result<Self, EmptyIoOutputBufferError> {

        // There must be some space to write into
        if data.capacity() == 0 {
            return Err(EmptyIoOutputBufferError { buffer: data });
        }

        Ok(Self(data))
    }

    pub fn as_bytes(&self) -> &BytesMut {
        &self.0
    }

    pub fn into_bytes(mut self, len: usize) -> Result<BytesMut, IoOutputBufferIntoBytesError> {

        let chunk_len = self.0.chunk_mut().len();

        unsafe { self.0.advance_mut(std::cmp::min(len, chunk_len)); }

        if len > chunk_len {
            return Err(IoOutputBufferIntoBytesError::BufferOverflow {
                requested: len,
                available: chunk_len,
                buffer: self.0
            });
        }

        Ok(self.0)
    }

    pub fn into_bytes_unchecked(self) -> BytesMut { self.0 }

    pub unsafe fn as_mut_ptr(&mut self) -> *mut u8 {
        self.0.chunk_mut().as_mut_ptr()
    }

    pub fn writable_len(&mut self) -> usize {
        self.0.chunk_mut().len()
    }

}


pub trait GenerateIoVecs {
    unsafe fn generate_iovecs(&self) -> Result<Vec<libc::iovec>, InvalidIoVecError>;
}

#[derive(Debug)]
pub enum IoVecInputSource {
    Single(Bytes),
    Multiple(Arc<Vec<Bytes>>),
}

#[derive(Debug, Error)]
#[error("The input buffer is empty.")]
pub struct EmptySingleVectoredInputBufferError {
    pub buffer: Bytes,
}

#[derive(Debug, Error)]
#[error("The input buffer is empty.")]
pub struct EmptyVectoredInputBufferError {
    pub buffer: Arc<Vec<Bytes>>,
}

#[derive(Debug)]
// Represents a non-contiguous, immutable buffer for writing from.
pub struct IoVecInputBuffer(IoVecInputSource);

impl IoVecInputBuffer {

    pub fn new_single(data: Bytes) -> Result<Self, EmptySingleVectoredInputBufferError> {
        if data.is_empty() {
            return Err(EmptySingleVectoredInputBufferError { buffer: data });
        }

        Ok(Self(IoVecInputSource::Single(data)))
    }

    pub fn new_multiple(data: Arc<Vec<Bytes>>) -> Result<Self, EmptyVectoredInputBufferError> {
        if data.is_empty() {
            return Err(EmptyVectoredInputBufferError { buffer: data });
        }

        Ok(Self(IoVecInputSource::Multiple(data)))
    }

    pub fn into_source(self) -> IoVecInputSource {
        self.0
    }

    pub fn as_source(&self) -> &IoVecInputSource {
        &self.0
    }

    pub fn try_as_bytes(&self) -> Option<&Bytes> {
        match &self.0 {
            IoVecInputSource::Single(bytes) => Some(bytes),
            IoVecInputSource::Multiple(_) => None,
        }
    }

    /*
        SAFETY: The caller must ensure that the source type is `Single`.
    */
    pub unsafe fn as_bytes(&self) -> &Bytes {
        match &self.0 {
            IoVecInputSource::Single(bytes) => bytes,
            IoVecInputSource::Multiple(_) => {
                panic!("Expected a single buffer, but got multiple.");
            }
        }
    }

    /*
        SAFETY: The caller must ensure that the source type is `Single`.
    */
    pub unsafe fn as_mut_bytes(&mut self) -> &mut Bytes {
        match &mut self.0 {
            IoVecInputSource::Single(bytes) => bytes,
            IoVecInputSource::Multiple(_) => {
                panic!("Expected a single buffer, but got multiple.");
            }
        }
    }

    pub fn try_as_mut_bytes(&mut self) -> Option<&mut Bytes> {
        match &mut self.0 {
            IoVecInputSource::Single(bytes) => Some(bytes),
            IoVecInputSource::Multiple(_) => None,
        }
    }

    pub fn try_into_bytes(self) -> Option<Bytes> {
        match self.0 {
            IoVecInputSource::Single(bytes) => Some(bytes),
            IoVecInputSource::Multiple(_) => None,
        }
    }

    /*
        SAFETY: The caller must ensure that the source type is `Single`.
    */
    pub unsafe fn into_bytes(self) -> Bytes {
        match self.0 {
            IoVecInputSource::Single(bytes) => bytes,
            IoVecInputSource::Multiple(_) => {
                panic!("Expected a single buffer, but got multiple.");
            }
        }
    }

    pub fn try_as_vec(&self) -> Option<&Vec<Bytes>> {
        match &self.0 {
            IoVecInputSource::Single(_) => None,
            IoVecInputSource::Multiple(vec) => Some(vec),
        }
    }

    /*
        SAFETY: The caller must ensure that the source type is `Multiple`.
    */
    pub unsafe fn as_vec(&self) -> &Vec<Bytes> {
        match &self.0 {
            IoVecInputSource::Single(_) => {
                panic!("Expected multiple buffers, but got a single one.");
            }
            IoVecInputSource::Multiple(vec) => vec,
        }
    }

    /*
        SAFETY: The caller must ensure that the source type is `Multiple`.
    */

    /*
        SAFETY: The caller must ensure that the source type is `Multiple`.
    */
    pub unsafe fn into_vec(self) -> Arc<Vec<Bytes>> {
        match self.0 {
            IoVecInputSource::Single(_) => {
                panic!("Expected multiple buffers, but got a single one.");
            }
            IoVecInputSource::Multiple(vec) => vec,
        }
    }
}

impl GenerateIoVecs for IoVecInputBuffer {
    unsafe fn generate_iovecs(&self) -> Result<Vec<libc::iovec>, InvalidIoVecError> {
        match self.0 {
            IoVecInputSource::Single(ref bytes) => {
                unsafe { Ok(bytes.generate_iovecs()?) }
            },
            IoVecInputSource::Multiple(ref vec) => {
                unsafe { Ok(vec.generate_iovecs()?) }
            },
        }
    }
}

#[derive(Debug)]
pub struct RecvMsgBuffers {
    data: Option<Vec<BytesMut>>,
    control: Option<BytesMut>,
}

impl Default for RecvMsgBuffers {
    fn default() -> Self {
        Self {
            data: None,
            control: None,
        }
    }
}

impl RecvMsgBuffers {
    pub fn new(data: Option<Vec<BytesMut>>, control: Option<BytesMut>) -> Self {
        Self {
            data,
            control,
        }
    }

    pub fn data(&self) -> Option<&Vec<BytesMut>> {
        self.data.as_ref()
    }

    pub fn data_mut(&mut self) -> Option<&mut Vec<BytesMut>> {
        self.data.as_mut()
    }

    pub fn control(&self) -> Option<&BytesMut> {
        self.control.as_ref()
    }

    pub fn control_mut(&mut self) -> Option<&mut BytesMut> {
        self.control.as_mut()
    }

    pub fn take_data(&mut self) -> Option<Vec<BytesMut>> {
        self.data.take()
    }

    pub fn take_control(&mut self) -> Option<BytesMut> {
        self.control.take()
    }

    pub fn add_data(&mut self, data: BytesMut) {
        if self.data.is_none() {
            self.data = Some(vec![data]);
        } else {
            self.data.as_mut().unwrap().push(data);
        }
    }

    pub fn extend_data(&mut self, data: Vec<BytesMut>) {
        if self.data.is_none() {
            self.data = Some(data);
        } else {
            self.data.as_mut().unwrap().extend(data);
        }
    }

    pub fn set_control(&mut self, control: BytesMut) {
        self.control = Some(control);
    }

    pub fn clear_data(&mut self) {
        self.data = None;
    }

    pub fn clear_control(&mut self) {
        self.control = None;
    }

    pub fn split(mut self) -> (Option<Vec<BytesMut>>, Option<BytesMut>) {
        let data = self.data.take();
        let control = self.control.take();
        (data, control)
    }

    pub (crate) fn prepare(self) -> Result<IoRecvMsgOutputBuffers, RecvMsgSubmissionError> {

        let data = match self.data {
            Some(data) => {
                match IoVecOutputBuffer::new(data) {
                    Ok(data) => Some(data),
                    Err(e) => {
                        let e_str = e.to_string();
                        return Err(RecvMsgSubmissionError {
                            buffers: RecvMsgBuffers {
                                data: Some(e.buffer),
                                control: self.control,
                            },
                            kind: RecvMsgSubmissionErrorKind::InvalidDataBuffer(e_str),
                        });
                    }
                }
            },
            None => None,
        };

        let control = match self.control {
            Some(control) => {
                match IoOutputBuffer::new(control) {
                    Ok(control) => Some(control),
                    Err(e) => {
                        let e_str = e.to_string();
                        return Err(RecvMsgSubmissionError {
                            buffers: RecvMsgBuffers {
                                data: data.map(|d| d.into_vec()),
                                control: Some(e.buffer),
                            },
                            kind: RecvMsgSubmissionErrorKind::InvalidControlBuffer(e_str),
                        });
                    }
                }
            },
            None => None,
        };

        Ok(IoRecvMsgOutputBuffers {
            data,
            control,
        })
    }

    pub fn refs(&self) -> RecvMsgBuffersRefs {
        RecvMsgBuffersRefs::new(self.data.as_ref(), self.control.as_ref())
    }
}

fn generate_iovecs_single(mut bytes: Bytes) -> Vec<libc::iovec> {
    let mut iovecs = Vec::new();

    while bytes.has_remaining() {
        let chunk = bytes.chunk();
        let len = chunk.len();

        if len == 0 { continue; }

        let iovec = libc::iovec {
            iov_base: chunk.as_ptr() as *mut libc::c_void,
            iov_len: len,
        };

        iovecs.push(iovec);
        bytes.advance(len);
    }

    iovecs
}

pub struct RecvMsgBuffersRefs<'a> {
    data: Option<&'a Vec<BytesMut>>,
    control: Option<&'a BytesMut>,
}

impl<'a> RecvMsgBuffersRefs<'a> {
    pub fn new(data: Option<&'a Vec<BytesMut>>, control: Option<&'a BytesMut>) -> Self {
        Self { data, control }
    }

    pub fn data(&self) -> Option<&'a Vec<BytesMut>> {
        self.data
    }

    pub fn control(&self) -> Option<&'a BytesMut> {
        self.control
    }
}

#[derive(Debug)]
pub struct IoRecvMsgOutputBuffers {
    pub data: Option<IoVecOutputBuffer>,
    pub control: Option<IoOutputBuffer>,
}

impl IoRecvMsgOutputBuffers {

    pub fn as_raw(&self) -> RecvMsgBuffersRefs {
        let data = self.data.as_ref().map(|b| b.as_vec());
        let control = self.control.as_ref().map(|b| b.as_bytes());
        RecvMsgBuffersRefs::new(data, control)
    }

    pub fn unwrap(self) -> RecvMsgBuffers {
        RecvMsgBuffers {
            data: self.data.map(|b| b.into_vec()),
            control: self.control.map(|b| b.into_bytes_unchecked()),
        }
    }
}

impl GenerateIoVecs for Bytes {
    unsafe fn generate_iovecs(&self) -> Result<Vec<libc::iovec>, InvalidIoVecError> {
        let iovecs = generate_iovecs_single(self.clone());
        assert_iovecs_valid(&iovecs)?;
        Ok(iovecs)
    }
}

impl GenerateIoVecs for Vec<Bytes> {
    unsafe fn generate_iovecs(&self) -> Result<Vec<libc::iovec>, InvalidIoVecError> {
        let mut iovecs = Vec::new();
        for bytes in self.iter() {
            let iovecs_single = generate_iovecs_single(bytes.clone());
            if iovecs_single.is_empty() { continue; }
            iovecs.extend(iovecs_single);
        }
        assert_iovecs_valid(&iovecs)?;
        Ok(iovecs)
    }
}

impl GenerateIoVecs for Vec<BytesMut> {
    unsafe fn generate_iovecs(&self) -> Result<Vec<libc::iovec>, InvalidIoVecError> {
        let mut iovecs = Vec::new();
        for bytes in self.iter() {

            let ptr = bytes.as_ptr() as *mut u8;

            let current_len = bytes.len();
            let cap = bytes.capacity();

            if cap <= current_len { continue; }
            let uninit_chunk_len = cap - current_len;

            let uninit_chunk_ptr_mut = unsafe {
                ptr.add(current_len) as *mut libc::c_void
            };

            let iovec = libc::iovec {
                iov_base: uninit_chunk_ptr_mut,
                iov_len: uninit_chunk_len,
            };

            iovecs.push(iovec);
        }
        assert_iovecs_valid(&iovecs)?;
        Ok(iovecs)
    }
}

#[derive(Debug, Error)]
pub enum IoVecOutputBufferIntoBytesError {

    #[error("Buffer overflow: requested {requested} bytes, but only {available} bytes available.")]
    BufferOverflow {
        requested: usize,
        available: usize,
        buffers: Vec<BytesMut>,
    }
}

impl IoVecOutputBufferIntoBytesError {
    pub fn as_buffers(&self) -> &Vec<BytesMut> {
        match self {
            IoVecOutputBufferIntoBytesError::BufferOverflow { buffers, .. } => buffers,
        }
    }

    pub fn into_buffers(self) -> Vec<BytesMut> {
        match self {
            IoVecOutputBufferIntoBytesError::BufferOverflow { buffers, .. } => buffers,
        }
    }
}

#[derive(Debug, Error)]
#[error("The output buffer is empty.")]
pub struct EmptyVectoredOutputBufferError {
    pub buffer: Vec<BytesMut>,
}

#[derive(Debug)]
// Represents a non-contiguous, mutable buffer for reading into.
pub struct IoVecOutputBuffer(Vec<BytesMut>);

impl IoVecOutputBuffer {
    pub fn new(data: Vec<BytesMut>) -> Result<Self, EmptyVectoredOutputBufferError> {

        // There must be some space to write into
        if data.is_empty() {
            return Err(EmptyVectoredOutputBufferError { buffer: data });
        }

        Ok(Self(data))
    }

    pub fn as_vec(&self) -> &Vec<BytesMut> {
        &self.0
    }

    pub fn into_vec(self) -> Vec<BytesMut> {
        self.0
    }

    pub fn into_bytes(mut self, total_bytes: usize) -> Result<Vec<BytesMut>, IoVecOutputBufferIntoBytesError> {
        let mut accounted_bytes = 0;

        for buffer in &mut self.0 {
            let uninit = buffer.chunk_mut();

            let bytes_in_this_buffer = total_bytes
                .saturating_sub(accounted_bytes)
                .min(uninit.len())
            ;

            unsafe { buffer.advance_mut(bytes_in_this_buffer); }
            accounted_bytes += bytes_in_this_buffer;
        }

        if accounted_bytes < total_bytes {
            return Err(IoVecOutputBufferIntoBytesError::BufferOverflow {
                requested: total_bytes,
                available: accounted_bytes,
                buffers: self.0
            });
        }

        Ok(self.0)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl GenerateIoVecs for IoVecOutputBuffer {
    unsafe fn generate_iovecs(&self) -> Result<Vec<libc::iovec>, InvalidIoVecError> {
        unsafe { self.0.generate_iovecs() }
    }
}

// Submission errors where the input is a Bytes
#[derive(Debug, Error)]
pub enum InputBufferSubmissionError {
    #[error("Invalid continguous input buffer: {0}")]
    InvalidInputBuffer(#[from] InvalidIoInputBufferError),

    #[error("Invalid non-contiguous input buffer: {0}")]
    InvalidIoVecInputBuffer(#[from] EmptySingleVectoredInputBufferError),

    #[error("The provided input buffers generated an invalid iovec: {source}")]
    InvalidIoVec {

        buffer: Bytes,

        #[source]
        source: InvalidIoVecError,
    }
}

impl InputBufferSubmissionError {

    pub fn as_buffer(&self) -> &Bytes {
        match self {
            InputBufferSubmissionError::InvalidInputBuffer(e) => &e.buffer,
            InputBufferSubmissionError::InvalidIoVecInputBuffer(e) => &e.buffer,
            InputBufferSubmissionError::InvalidIoVec { buffer, .. } => buffer,
        }
    }

    pub fn into_buffer(self) -> Bytes {
        match self {
            InputBufferSubmissionError::InvalidInputBuffer(e) => e.buffer,
            InputBufferSubmissionError::InvalidIoVecInputBuffer(e) => e.buffer,
            InputBufferSubmissionError::InvalidIoVec { buffer, .. } => buffer,
        }
    }
}

// Submission errors where the input is a Vec<Bytes>
#[derive(Debug, Error)]
pub enum InputBufferVecSubmissionError {
    #[error("Invalid input buffer array: {0}")]
    InvalidIoVecInputBuffer(#[from] EmptyVectoredInputBufferError),

    #[error("The provided input buffers generated an invalid iovec: {source}")]
    InvalidIoVec {

        buffer: Arc<Vec<Bytes>>,

        #[source]
        source: InvalidIoVecError,
    }
}

impl InputBufferVecSubmissionError {

    pub fn as_buffers(&self) -> &Vec<Bytes> {
        match self {
            InputBufferVecSubmissionError::InvalidIoVecInputBuffer(e) => &e.buffer,
            InputBufferVecSubmissionError::InvalidIoVec { buffer, .. } => buffer,
        }
    }

    pub fn into_buffers(self) -> Arc<Vec<Bytes>> {
        match self {
            InputBufferVecSubmissionError::InvalidIoVecInputBuffer(e) => e.buffer,
            InputBufferVecSubmissionError::InvalidIoVec { buffer, .. } => buffer,
        }
    }
}

// Submission errors where the output is a BytesMut
#[derive(Debug, Error)]
pub enum OutputBufferSubmissionError {
    #[error("Invalid continguous output buffer: {0}")]
    InvalidOutputBuffer(#[from] EmptyIoOutputBufferError),

    #[error("The provided output buffers generated an invalid iovec: {source}")]
    InvalidIoVec {

        buffer: BytesMut,

        #[source]
        source: InvalidIoVecError,
    }
}

impl OutputBufferSubmissionError {

    pub fn as_buffer(&self) -> &BytesMut {
        match self {
            OutputBufferSubmissionError::InvalidOutputBuffer(e) => &e.buffer,
            OutputBufferSubmissionError::InvalidIoVec { buffer, .. } => buffer,
        }
    }

    pub fn into_buffer(self) -> BytesMut {
        match self {
            OutputBufferSubmissionError::InvalidOutputBuffer(e) => e.buffer,
            OutputBufferSubmissionError::InvalidIoVec { buffer, .. } => buffer,
        }
    }
}

// Submission errors where the output is a Vec<BytesMut>
#[derive(Debug, Error)]
pub enum OutputBufferVecSubmissionError {
    #[error("Invalid output buffer array: {0}")]
    InvalidOutputBuffer(#[from] EmptyVectoredOutputBufferError),

    #[error("The provided output buffers generated an invalid iovec: {source}")]
    InvalidIoVec {

        buffer: Vec<BytesMut>,

        #[source]
        source: InvalidIoVecError,
    }
}

impl OutputBufferVecSubmissionError {

    pub fn as_buffers(&self) -> &Vec<BytesMut> {
        match self {
            OutputBufferVecSubmissionError::InvalidOutputBuffer(e) => &e.buffer,
            OutputBufferVecSubmissionError::InvalidIoVec { buffer, .. } => buffer,
        }
    }

    pub fn into_buffers(self) -> Vec<BytesMut> {
        match self {
            OutputBufferVecSubmissionError::InvalidOutputBuffer(e) => e.buffer,
            OutputBufferVecSubmissionError::InvalidIoVec { buffer, .. } => buffer,
        }
    }
}

#[derive(Debug, Error)]
pub enum RecvMsgSubmissionErrorKind {
    #[error("Invalid data buffer: {0}")]
    InvalidDataBuffer(String),

    #[error("Invalid control buffer: {0}")]
    InvalidControlBuffer(String),

    #[error("Failed to generate iovecs for data buffer: {0}")]
    InvalidIoVec(#[source] InvalidIoVecError),
}

#[derive(Debug, Error)]
#[error("Failed to submit recvmsg buffers: {kind}")]
pub struct RecvMsgSubmissionError {
    
    pub buffers: RecvMsgBuffers,

    #[source]
    pub kind: RecvMsgSubmissionErrorKind,
}

impl RecvMsgSubmissionError {
    pub fn as_buffers(&self) -> &RecvMsgBuffers {
        &self.buffers
    }

    pub fn into_buffers(self) -> RecvMsgBuffers {
        self.buffers
    }

    pub fn as_buffers_mut(&mut self) -> &mut RecvMsgBuffers {
        &mut self.buffers
    }
}