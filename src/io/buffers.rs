use bytes::{Buf, BufMut, Bytes, BytesMut};

use super::IoOperationError;

// INPUT BUFFER


// Represents a contiguous, immutable buffer for writing from.
pub struct IoInputBuffer(Bytes);

impl IoInputBuffer {
    pub fn new(data: Bytes) -> Result<Self, IoOperationError> {

        // Writing empty data is incoherent
        if data.is_empty() { 
            return Err(IoOperationError::NoData.into()); 
        }

        /* 
            MUST be a contiguous buffer

            For non-contiguous writes, look at IoVecInputBuffer / writev
        
        */
        if data.chunk().len() != data.len() {
            return Err(IoOperationError::InvalidPtr.into());
        }

        Ok(Self(data))
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

// Represents a contiguous, mutable buffer for reading into.
pub struct IoOutputBuffer(BytesMut);

impl IoOutputBuffer {

    pub fn with_capacity(capacity: usize) -> Self {
        Self(BytesMut::with_capacity(capacity))
    }
    
    pub fn new(data: BytesMut) -> Result<Self, IoOperationError> {

        // There must be some space to write into
        if data.capacity() == 0 {
            return Err(IoOperationError::InvalidPtr.into());
        }

        Ok(Self(data))
    }

    pub fn into_bytes(mut self, len: usize) -> Result<BytesMut, IoOperationError> {

        let chunk = self.0.chunk_mut();

        if len > chunk.len() {
            return Err(IoOperationError::BufferOverflow(
                chunk.len(), 
                len
            ).into());
        }

        unsafe { self.0.advance_mut(len); }

        Ok(self.0)
    }

    pub unsafe fn as_mut_ptr(&mut self) -> *mut u8 {
        self.0.chunk_mut().as_mut_ptr()
    }

    pub fn writable_len(&mut self) -> usize {
        self.0.chunk_mut().len()
    }

}


pub trait GenerateIoVecs {
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoOperationError>;
}

pub enum IoVecInputSource {
    Single(Bytes),
    Multiple(Vec<Bytes>),
}

// Represents a non-contiguous, immutable buffer for writing from.
pub struct IoVecInputBuffer(IoVecInputSource);

impl IoVecInputBuffer {

    pub fn new_single(data: Bytes) -> Result<Self, IoOperationError> {
        if data.is_empty() {
            return Err(IoOperationError::NoData.into());
        }

        Ok(Self(IoVecInputSource::Single(data)))
    }

    pub fn new_multiple(data: Vec<Bytes>) -> Result<Self, IoOperationError> {
        if data.is_empty() {
            return Err(IoOperationError::NoData.into());
        }

        for bytes in &data {
            if bytes.is_empty() {
                return Err(IoOperationError::NoData.into());
            }
        }

        Ok(Self(IoVecInputSource::Multiple(data)))
    }

    pub fn into_source(self) -> IoVecInputSource {
        self.0
    }

    fn generate_iovecs_single(mut bytes: Bytes) -> Vec<libc::iovec> {
        let mut iovecs = Vec::new();

        while bytes.has_remaining() {
            let chunk = bytes.chunk();
            let len = chunk.len();

            if len == 0 { break; }

            let iovec = libc::iovec {
                iov_base: chunk.as_ptr() as *mut libc::c_void,
                iov_len: len,
            };

            iovecs.push(iovec);
            bytes.advance(len);
        }

        iovecs
    }
}

impl GenerateIoVecs for IoVecInputBuffer {
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoOperationError> {
        match self.0 {
            IoVecInputSource::Single(ref mut bytes) => {
                let iovecs = Self::generate_iovecs_single(bytes.clone());
                if iovecs.is_empty() {
                    return Err(IoOperationError::NoData.into());
                }
                Ok(iovecs)
            },
            IoVecInputSource::Multiple(ref mut vec) => {
                let mut iovecs = Vec::new();
                for bytes in vec.iter_mut() {
                    let iovecs_single = Self::generate_iovecs_single(bytes.clone());
                    iovecs.extend(iovecs_single);
                    if iovecs.is_empty() { break; }
                }
                if iovecs.is_empty() {
                    return Err(IoOperationError::NoData.into());
                }
                Ok(iovecs)
            },
        }
    }
}

// Represents a non-contiguous, mutable buffer for reading into.
pub struct IoVecOutputBuffer(Vec<BytesMut>);

impl IoVecOutputBuffer {
    pub fn new(data: Vec<BytesMut>) -> Result<Self, IoOperationError> {

        // There must be some space to write into
        if data.is_empty() {
            return Err(IoOperationError::InvalidPtr.into());
        }

        Ok(Self(data))
    }

    pub fn into_bytes(mut self, total_bytes: usize) -> Result<Vec<BytesMut>, IoOperationError> {
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
            return Err(IoOperationError::BufferOverflow(
                accounted_bytes, 
                total_bytes
            ).into());
        }

        Ok(self.0)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl GenerateIoVecs for IoVecOutputBuffer {
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoOperationError> {
        let mut iovecs = Vec::new();

        for buffer in &mut self.0 {
            let chunk = buffer.chunk_mut();
            let len = chunk.len();

            if len == 0 { break; }

            let iovec = libc::iovec {
                iov_base: chunk.as_mut_ptr() as *mut libc::c_void,
                iov_len: len,
            };

            iovecs.push(iovec);
        }

        if iovecs.is_empty() {
            return Err(IoOperationError::NoData.into());
        }

        Ok(iovecs)
    }
}