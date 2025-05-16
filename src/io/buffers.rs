use bytes::{Buf, BufMut, Bytes, BytesMut};
use once_cell::sync::Lazy;
use thiserror::Error;

use super::IoSubmissionError;

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

fn assert_iovecs_valid(iovecs: &[libc::iovec]) -> Result<(), IoSubmissionError> {
    if iovecs.is_empty() {
        Err(IoSubmissionError::NoWritableMemory)
    } else if iovecs.len() > *IOV_MAX_LIMIT {
        Err(IoSubmissionError::TooManyIoVecs {
            requested: iovecs.len(),
            max: *IOV_MAX_LIMIT,
        })
    } else { Ok(()) }
}

// INPUT BUFFER
#[derive(Debug)]
// Represents a contiguous, immutable buffer for writing from.
pub struct IoInputBuffer(Bytes);

impl IoInputBuffer {
    pub fn new(data: Bytes) -> Result<Self, IoSubmissionError> {

        // Writing empty data is incoherent
        if data.is_empty() { 
            return Err(IoSubmissionError::EmptyInputBuffer(data)); 
        }

        /* 
            MUST be a contiguous buffer

            For non-contiguous writes, look at IoVecInputBuffer / writev
        
        */
        if data.chunk().len() != data.len() {
            return Err(IoSubmissionError::NonContiguousInputBuffer(data));
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

#[derive(Debug)]
// Represents a contiguous, mutable buffer for reading into.
pub struct IoOutputBuffer(BytesMut);

impl IoOutputBuffer {

    pub fn with_capacity(capacity: usize) -> Self {
        Self(BytesMut::with_capacity(capacity))
    }
    
    pub fn new(data: BytesMut) -> Result<Self, IoSubmissionError> {

        // There must be some space to write into
        if data.capacity() == 0 {
            return Err(IoSubmissionError::EmptyOutputBuffer(data));
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
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoSubmissionError>;
}

#[derive(Debug)]
pub enum IoVecInputSource {
    Single(Bytes),
    Multiple(Vec<Bytes>),
}

#[derive(Debug)]
// Represents a non-contiguous, immutable buffer for writing from.
pub struct IoVecInputBuffer(IoVecInputSource);

impl IoVecInputBuffer {

    pub fn new_single(data: Bytes) -> Result<Self, IoSubmissionError> {
        if data.is_empty() {
            return Err(IoSubmissionError::EmptyInputBuffer(data));
        }

        Ok(Self(IoVecInputSource::Single(data)))
    }

    pub fn new_multiple(data: Vec<Bytes>) -> Result<Self, IoSubmissionError> {
        if data.is_empty() {
            return Err(IoSubmissionError::EmptyVectoredInputBuffer(data));
        }

        Ok(Self(IoVecInputSource::Multiple(data)))
    }

    pub fn into_source(self) -> IoVecInputSource {
        self.0
    }

    pub fn as_source(&self) -> &IoVecInputSource {
        &self.0
    }
}

impl GenerateIoVecs for IoVecInputBuffer {
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoSubmissionError> {
        match self.0 {
            IoVecInputSource::Single(ref mut bytes) => {
                unsafe { Ok(bytes.generate_iovecs()?) }
            },
            IoVecInputSource::Multiple(ref mut vec) => {
                unsafe { Ok(vec.generate_iovecs()?) }
            },
        }
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

impl GenerateIoVecs for Bytes {
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoSubmissionError> {
        let iovecs = generate_iovecs_single(self.clone());
        assert_iovecs_valid(&iovecs)?;
        Ok(iovecs)
    }
}

impl GenerateIoVecs for Vec<Bytes> {
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoSubmissionError> {
        let mut iovecs = Vec::new();
        for bytes in self.iter_mut() {
            let iovecs_single = generate_iovecs_single(bytes.clone());
            if iovecs_single.is_empty() { continue; }
            iovecs.extend(iovecs_single);
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

#[derive(Debug)]
// Represents a non-contiguous, mutable buffer for reading into.
pub struct IoVecOutputBuffer(Vec<BytesMut>);

impl IoVecOutputBuffer {
    pub fn new(data: Vec<BytesMut>) -> Result<Self, IoSubmissionError> {

        // There must be some space to write into
        if data.is_empty() {
            return Err(IoSubmissionError::EmptyVectoredOutputBuffer(data));
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
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoSubmissionError> {
        let mut iovecs = Vec::with_capacity(self.0.len());

        for buffer in &mut self.0 {
            let chunk = buffer.chunk_mut();
            let len = chunk.len();

            if len == 0 { continue; }

            let iovec = libc::iovec {
                iov_base: chunk.as_mut_ptr() as *mut libc::c_void,
                iov_len: len,
            };

            iovecs.push(iovec);
        }

        assert_iovecs_valid(&iovecs)?;

        Ok(iovecs)
    }
}