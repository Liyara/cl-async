use std::{
    fmt, 
    os::fd::{
        AsRawFd, 
        RawFd
    }
};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct EventSource(RawFd);

impl EventSource {
    pub fn new<T>(source: &T) -> Self where T: AsRawFd {
        Self(source.as_raw_fd())
    }
}

impl AsRawFd for EventSource {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl fmt::Display for EventSource {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}