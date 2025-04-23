use parking_lot::Mutex;

use crate::io::{GenerateIoVecs, IoDoubleInputBuffer, IoError, IoResult};

struct IoWritevDataInner {
    _iovec: IoDoubleInputBuffer,
    _iovec_ptr: Vec<libc::iovec>
}

impl IoWritevDataInner {
    fn new(mut _iovec: IoDoubleInputBuffer) -> Result<Self, IoError> {
        
        let _iovec_ptr = unsafe { _iovec.generate_iovecs()? };

        Ok(Self {
            _iovec,
            _iovec_ptr
        })
    }
}

unsafe impl Send for IoWritevDataInner {}

pub struct IoWritevData {
    inner: Mutex<IoWritevDataInner>,
    offset: usize,
}

impl IoWritevData {
    pub fn new(iovec: IoDoubleInputBuffer, offset: usize) -> IoResult<Self> {

        Ok(Self {
            inner: Mutex::new(IoWritevDataInner::new(iovec)?),
            offset
        })
    }
}

impl super::CompletableOperation for IoWritevData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletionResult {
        Ok(crate::io::IoCompletion::Write(crate::io::completion_data::IoWriteCompletion {
            bytes_written: result_code as usize,
        }))
    }
}

impl super::AsUringEntry for IoWritevData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        let iovec = &mut self.inner.lock()._iovec_ptr;
        io_uring::opcode::Writev::new(
            io_uring::types::Fd(fd),
            iovec.as_mut_ptr(),
            iovec.len() as _,
        ).offset(self.offset as u64)
        .build().user_data(key.as_u64())
    }
}


