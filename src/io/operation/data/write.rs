use crate::io::IoInputBuffer;

pub struct IoWriteData {
    buffer: IoInputBuffer,
    offset: usize,
}

impl IoWriteData {
    pub fn new(write_data: IoInputBuffer, offset: usize) -> Self {
        Self {
            buffer: write_data,
            offset,
        }
    }

    pub fn buffer(&self) -> &IoInputBuffer {
        &self.buffer
    }

    pub fn into_buffer(self) -> IoInputBuffer {
        self.buffer
    }
}

impl super::CompletableOperation for IoWriteData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletion {
        crate::io::IoCompletion::Write(crate::io::completion_data::IoWriteCompletion {
            bytes_written: result_code as usize,
        })
    }
}

impl super::AsUringEntry for IoWriteData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        unsafe {
            io_uring::opcode::Write::new(
                io_uring::types::Fd(fd),
                self.buffer.as_ptr(),
                self.buffer.readable_len() as u32,
            ).offset(self.offset as _)
            .build().user_data(key.as_u64())
        }
    }
}