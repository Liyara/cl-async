use crate::io::{buffers::IoVecOutputBuffer, failure::{data::IoMultiReadFailure, IoFailure}, GenerateIoVecs, IoSubmissionError};

struct IoReadvDataInner {
    iovec: Option<IoVecOutputBuffer>,
    _iovec_ptr: Vec<libc::iovec>
}

impl IoReadvDataInner {
    fn new(mut iovec: IoVecOutputBuffer) -> Result<Self, IoSubmissionError> {
        
        let _iovec_ptr = unsafe { iovec.generate_iovecs()? };

        Ok(Self {
            iovec: Some(iovec),
            _iovec_ptr
        })
    }
}

unsafe impl Send for IoReadvDataInner {}
unsafe impl Sync for IoReadvDataInner {}

pub struct IoReadvData {
    inner: IoReadvDataInner,
    offset: usize,
}

impl IoReadvData {
    pub fn new(iovec: IoVecOutputBuffer, offset: usize) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            inner: IoReadvDataInner::new(iovec)?,
            offset
        })
    }

    pub fn buffers(&self) -> Option<&IoVecOutputBuffer> {
        self.inner.iovec.as_ref()
    }

    pub fn into_buffers(mut self) -> Option<IoVecOutputBuffer> {
        self.inner.iovec.take()
    }
}     

impl super::CompletableOperation for IoReadvData {

    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletion {

        let bytes_read = result_code as usize;
        
        let buffers = self.inner.iovec.take().map(|b| {
            match b.into_bytes(bytes_read) {
                Ok(buffer) => buffer,
                Err(e) => {
                    warn!("cl-async: readv(): Issue while advancing buffer: {e}");
                    e.into_buffers()
                }
            }
        }).unwrap_or({
            warn!("cl-async: readv(): Expected buffer but got None; returning empty buffer vec.");
            Vec::new()
        });

        crate::io::IoCompletion::MultiRead(crate::io::completion_data::IoMultiReadCompletion {
            buffers,
            bytes_read,
        })
    }

    fn get_failure(&mut self) -> crate::io::failure::IoFailure {
        IoFailure::MultiRead(IoMultiReadFailure {
            buffers: self.inner.iovec.take().map(|b| {
                b.into_vec()
            }).unwrap_or({
                warn!("cl-async: readv(): Expected buffer but got None; returning empty buffer vec.");
                Vec::new()
            }),
        })
    }
}

impl super::AsUringEntry for IoReadvData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        let inner = &mut self.inner;
        io_uring::opcode::Readv::new(
            io_uring::types::Fd(fd),
            inner._iovec_ptr.as_ptr(),
            inner._iovec_ptr.len() as u32,
        ).offset(self.offset as u64)
        .build().user_data(key.as_u64())
    }
}