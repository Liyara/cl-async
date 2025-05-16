use std::time::Duration;

pub struct IoTimeoutData {
    ts: Box<libc::timespec>,
}

impl IoTimeoutData {
    pub fn new(duration: Duration) -> Self {
        Self {
            ts: Box::new(libc::timespec {
                tv_sec: duration.as_secs() as i64,
                tv_nsec: duration.subsec_nanos() as i64,
            }),
        }
    }
}

impl super::CompletableOperation for IoTimeoutData {}

impl super::AsUringEntry for IoTimeoutData {
    fn as_uring_entry(&mut self, _: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::Timeout::new(
            self.ts.as_mut() as *const libc::timespec as *const io_uring::types::Timespec,
        ).build().user_data(key.as_u64())
    }
}