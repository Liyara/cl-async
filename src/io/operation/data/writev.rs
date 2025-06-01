use crate::io::{buffers::{IoVecInputBuffer, IoVecInputSource}, GenerateIoVecs, InputBufferSubmissionError, InputBufferVecSubmissionError, IoSubmissionError};

struct IoWritevDataInner {
    _iovec: IoVecInputBuffer,
    _iovec_ptr: Vec<libc::iovec>
}

impl IoWritevDataInner {
    fn new(mut _iovec: IoVecInputBuffer) -> Result<Self, IoSubmissionError> {

        let is_single_source = match _iovec.as_source() {
            IoVecInputSource::Single(_) => true,
            IoVecInputSource::Multiple(_) => false,
        };
        
        let gen_ret = unsafe { _iovec.generate_iovecs() };

        let _iovec_ptr = match gen_ret {
            Ok(v) => v,
            Err(e) => {
                let submission_error = if is_single_source {
                    IoSubmissionError::from(
                        InputBufferSubmissionError::InvalidIoVec {
                            buffer: unsafe { _iovec.into_bytes() },
                            source: e,
                        }
                    )
                } else {
                    IoSubmissionError::from(
                        InputBufferVecSubmissionError::InvalidIoVec {
                            source: e,
                        }
                    )
                };

                return Err(submission_error);
            }
        };

        Ok(Self {
            _iovec,
            _iovec_ptr
        })
    }
}

unsafe impl Send for IoWritevDataInner {}
unsafe impl Sync for IoWritevDataInner {}

pub struct IoWritevData {
    inner: IoWritevDataInner,
    offset: usize,
}

impl IoWritevData {
    pub fn new(iovec: IoVecInputBuffer, offset: usize) -> Result<Self, IoSubmissionError> {

        Ok(Self {
            inner: IoWritevDataInner::new(iovec)?,
            offset
        })
    }

    pub fn buffers(&self) -> &IoVecInputBuffer {
        &self.inner._iovec
    }

    pub fn into_buffers(self) -> IoVecInputBuffer {
        self.inner._iovec
    }
}

impl super::CompletableOperation for IoWritevData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletion {
        crate::io::IoCompletion::Write(crate::io::completion_data::IoWriteCompletion {
            bytes_written: result_code as usize,
        })
    }
}

impl super::AsUringEntry for IoWritevData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        let iovec = &mut self.inner._iovec_ptr;
        io_uring::opcode::Writev::new(
            io_uring::types::Fd(fd),
            iovec.as_mut_ptr(),
            iovec.len() as _,
        ).offset(self.offset as u64)
        .build().user_data(key.as_u64())
    }
}


