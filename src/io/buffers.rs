use super::IoOperationError;


pub trait IoBuffer : TryFrom<Vec<u8>, Error=IoOperationError> {
    fn buffer_limit(&self) -> usize;
    unsafe fn as_ptr(&self) -> *const u8;
    unsafe fn as_mut_ptr(&mut self) -> *mut u8;
}

// INPUT BUFFER

pub struct IoInputBuffer(Vec<u8>);

impl IoInputBuffer {
    pub fn new(data: Vec<u8>) -> Result<Self, IoOperationError> {

        if data.is_empty() { 
            return Err(IoOperationError::NoData.into()); 
        }

        Ok(Self(data))
    }

    pub fn into_vec(self) -> Vec<u8> { self.0 }
}

impl IoBuffer for IoInputBuffer {
    fn buffer_limit(&self) -> usize {
        self.0.len()
    }

    unsafe fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }

    unsafe fn as_mut_ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr()
    }
}

impl TryFrom<Vec<u8>> for IoInputBuffer {
    type Error = IoOperationError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl AsRef<[u8]> for IoInputBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsMut<[u8]> for IoInputBuffer {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

// OUTPUT BUFFER

pub struct IoOutputBuffer(Vec<u8>);

impl IoOutputBuffer {

    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }
    
    pub fn new(data: Vec<u8>) -> Result<Self, IoOperationError> {

        if data.capacity() == 0 {
            return Err(IoOperationError::InvalidPtr.into());
        }

        Ok(Self(data))
    }

    pub fn into_vec(mut self, len: usize) -> Result<Vec<u8>, IoOperationError> {

        if len > self.0.capacity() {
            return Err(IoOperationError::BufferOverflow(
                self.0.capacity(), 
                len
            ).into());
        }

        unsafe { self.0.set_len(len); }

        Ok(self.0)
    }

    pub fn write(&mut self, data: &[u8], offset: usize) -> Result<usize, IoOperationError> {

        if self.0.capacity() < data.len() + offset {
            return Err(IoOperationError::BufferOverflow(
                self.0.capacity(), 
                data.len()
            ).into());
        }

        unsafe {
            std::ptr::copy(
                data.as_ptr(),
                self.0.as_mut_ptr().add(offset),
                data.len()
            );
        }

        Ok(data.len())
    }

}

impl IoBuffer for IoOutputBuffer {
    fn buffer_limit(&self) -> usize {
        self.0.capacity()
    }

    unsafe fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }

    unsafe fn as_mut_ptr(&mut self) -> *mut u8 {
        self.0.as_mut_ptr()
    }
}

impl TryFrom<Vec<u8>> for IoOutputBuffer {
    type Error = IoOperationError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

pub trait GenerateIoVecs {
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoOperationError>;
}

pub trait IoDoubleBuffer: GenerateIoVecs + TryFrom<Vec<Vec<u8>>, Error=IoOperationError> {
    fn buffer_limit(&self, index: usize) -> Result<usize, IoOperationError>;
    fn len(&self) -> usize;
    unsafe fn as_ptr(&self, index: usize) -> Result<*const u8, IoOperationError>;
    unsafe fn as_mut_ptr(&mut self, index: usize) -> Result<*mut u8, IoOperationError>;
}

fn generate_iovecs<T: IoDoubleBuffer>(buffers: &mut T) -> Result<Vec<libc::iovec>, IoOperationError> {
    let mut iovecs = Vec::with_capacity(buffers.len());
    for i in 0..buffers.len() {
        let iov = libc::iovec {
            iov_base: unsafe { buffers.as_mut_ptr(i)? as *mut libc::c_void },
            iov_len: buffers.buffer_limit(i)? as libc::size_t,
        };
        iovecs.push(iov);
    }
    Ok(iovecs)
}

// DOUBLE INPUT BUFFER

pub struct IoDoubleInputBuffer(Vec<Vec<u8>>);

impl IoDoubleInputBuffer {

    pub fn new(buffers: Vec<Vec<u8>>) -> Result<Self, IoOperationError> {

        if buffers.is_empty() {
            return Err(IoOperationError::NoData.into());
        }

        for buffer in &buffers {
            if buffer.is_empty() {
                return Err(IoOperationError::NoData.into());
            }
        }

        Ok(Self(buffers))
    }

    pub fn into_vec(self) -> Vec<Vec<u8>> { self.0 }
}

impl GenerateIoVecs for IoDoubleInputBuffer {
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoOperationError> {
        generate_iovecs(self)
    }
}

impl IoDoubleBuffer for IoDoubleInputBuffer {

    fn buffer_limit(&self, index: usize) -> Result<usize, IoOperationError> {
        if index >= self.0.len() {
            return Err(IoOperationError::OutOfBounds(index).into());
        }

        Ok(self.0[index].len())
    }

    fn len(&self) -> usize { self.0.len() }

    unsafe fn as_ptr(&self, index: usize) -> Result<*const u8, IoOperationError> {
        if index >= self.0.len() {
            return Err(IoOperationError::OutOfBounds(index).into());
        }

        Ok(self.0[index].as_ptr())
    }

    unsafe fn as_mut_ptr(&mut self, index: usize) -> Result<*mut u8, IoOperationError> {
        if index >= self.0.len() {
            return Err(IoOperationError::OutOfBounds(index).into());
        }

        Ok(self.0[index].as_mut_ptr())
    }
}

impl AsRef<[Vec<u8>]> for IoDoubleInputBuffer {
    fn as_ref(&self) -> &[Vec<u8>] {
        &self.0
    }
}

impl AsMut<[Vec<u8>]> for IoDoubleInputBuffer {
    fn as_mut(&mut self) -> &mut [Vec<u8>] {
        &mut self.0
    }
}

impl TryFrom<Vec<Vec<u8>>> for IoDoubleInputBuffer {
    type Error = IoOperationError;

    fn try_from(value: Vec<Vec<u8>>) -> Result<Self, Self::Error> {
        IoDoubleInputBuffer::new(value)
    }
}

// DOUBLE OUTPUT BUFFER

pub struct IoDoubleOutputBuffer(Vec<Vec<u8>>);

impl IoDoubleOutputBuffer {

    pub fn new(buffers: Vec<Vec<u8>>) -> Result<Self, IoOperationError> {

        if buffers.is_empty() {
            return Err(IoOperationError::NoData.into());
        }

        for buffer in &buffers {
            if buffer.capacity() == 0 {
                return Err(IoOperationError::InvalidPtr.into());
            }
        }

        Ok(Self(buffers))
    }

    pub fn into_vec(mut self, total_bytes: usize) -> Result<Vec<Vec<u8>>, IoOperationError> {
        let mut accounted_bytes = 0;

        for buffer in &mut self.0 {
            let bytes_in_this_buffer = total_bytes
                .saturating_sub(accounted_bytes)
                .min(buffer.capacity())
            ;
    
            unsafe { buffer.set_len(bytes_in_this_buffer); }
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
}

impl GenerateIoVecs for IoDoubleOutputBuffer {
    unsafe fn generate_iovecs(&mut self) -> Result<Vec<libc::iovec>, IoOperationError> {
        generate_iovecs(self)
    }
}

impl IoDoubleBuffer for IoDoubleOutputBuffer {

    fn buffer_limit(&self, index: usize) -> Result<usize, IoOperationError> {
        if index >= self.0.len() {
            return Err(IoOperationError::OutOfBounds(index).into());
        }

        Ok(self.0[index].capacity())
    }

    fn len(&self) -> usize { self.0.len() }

    unsafe fn as_ptr(&self, index: usize) -> Result<*const u8, IoOperationError> {
        if index >= self.0.len() {
            return Err(IoOperationError::OutOfBounds(index).into());
        }

        Ok(self.0[index].as_ptr())
    }

    unsafe fn as_mut_ptr(&mut self, index: usize) -> Result<*mut u8, IoOperationError> {
        if index >= self.0.len() {
            return Err(IoOperationError::OutOfBounds(index).into());
        }

        Ok(self.0[index].as_mut_ptr())
    }
}

impl TryFrom<Vec<Vec<u8>>> for IoDoubleOutputBuffer {
    type Error = IoOperationError;

    fn try_from(value: Vec<Vec<u8>>) -> Result<Self, Self::Error> {
        IoDoubleOutputBuffer::new(value)
    }
}

