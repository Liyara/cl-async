use parking_lot::Mutex;
use crate::io::{buffers::{IoDoubleBuffer, IoDoubleOutputBuffer}, GenerateIoVecs, IoResult};

struct IoReadvDataInner {
    iovec: Option<IoDoubleOutputBuffer>,
    _iovec_ptr: Vec<libc::iovec>
}

impl IoReadvDataInner {
    fn new(mut iovec: IoDoubleOutputBuffer) -> IoResult<Self> {
        
        let _iovec_ptr = unsafe { iovec.generate_iovecs()? };

        Ok(Self {
            iovec: Some(iovec),
            _iovec_ptr
        })
    }
}

unsafe impl Send for IoReadvDataInner {}

pub struct IoReadvData {
    inner: Mutex<IoReadvDataInner>,
    offset: usize,
}

impl IoReadvData {
    pub fn new(iovec: IoDoubleOutputBuffer, offset: usize) -> IoResult<Self> {
        Ok(Self {
            inner: Mutex::new(IoReadvDataInner::new(iovec)?),
            offset
        })
    }
}     

impl super::CompletableOperation for IoReadvData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletionResult {
        
        let data = match self.inner.lock().iovec.take() {
            Some(iovec) => {
                iovec.into_vec(result_code as usize)?
            },
            None => {
                return Err(crate::io::IoOperationError::NoData);
            }
        };

        Ok(crate::io::IoCompletion::MultiRead(crate::io::completion_data::IoMultiReadCompletion {
            data,
        }))
    }
}

impl super::AsUringEntry for IoReadvData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        let inner = self.inner.lock();
        let iovec = inner.iovec.as_ref().unwrap();
        io_uring::opcode::Readv::new(
            io_uring::types::Fd(fd),
            inner._iovec_ptr.as_ptr(),
            iovec.len() as u32
        ).offset(self.offset as u64)
        .build().user_data(key.as_u64())
    }
}