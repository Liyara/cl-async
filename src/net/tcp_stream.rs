use std::{os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd}, sync::Arc};

use crate::io::fs::File;

use super::{Address, NetworkError, SocketConfigurable, SocketOption};

#[derive(Debug, Clone)]
pub struct TcpStream {
    fd: Arc<OwnedFd>,
    addr: Address
}

pub enum ReadResult {
    Closed, // No data, and the connection will not send any more data
    Received(usize), // The connection sent data
    WouldBlock // No data, but the connection may send more data at a later time
}

pub enum WriteResult {
    Sent, // The data was sent
    Partial(usize), // Some or all of the data was not sent
    WouldBlock // The data was unsable to send because the OS buffer is full
}

#[repr(i32)]
pub enum ShutdownType {
    Read = libc::SHUT_RD,
    Write = libc::SHUT_WR,
    Both = libc::SHUT_RDWR,
}

impl TcpStream {

    pub (super) fn new(fd: RawFd, addr: libc::sockaddr_in) -> Self {
        Self {
            fd: Arc::new(unsafe { OwnedFd::from_raw_fd(fd) }),
            addr: addr.into()
        }
    }

    pub fn address(&self) -> &Address {
        &self.addr
    }

    pub fn read(&self, buf: &mut [u8]) -> Result<ReadResult, NetworkError> {
        let result = syscall!(recv(
            self.fd.as_raw_fd(),
            buf.as_mut_ptr() as *mut libc::c_void,
            buf.len(),
            0
        ));

        if let Err(os_err) = result {
            let raw_err = os_err.raw_os_error();
            if
                raw_err == Some(libc::EAGAIN) ||
                raw_err == Some(libc::EWOULDBLOCK)
            { Ok(ReadResult::WouldBlock) }
            else { Err(NetworkError::SocketReadError(os_err.into())) }
        } else {
            let bytes = result.unwrap();
            if bytes == 0 { Ok(ReadResult::Closed) }
            else { Ok(ReadResult::Received(bytes as usize)) }
        }
    }

    pub fn write(&self, data: &[u8]) -> Result<WriteResult, NetworkError> {
        let result = syscall!(send(
            self.fd.as_raw_fd(),
            data.as_ptr() as *const libc::c_void,
            data.len(),
            0
        ));

        if let Err(os_err) = result {
            let raw_err = os_err.raw_os_error();
            if
                raw_err == Some(libc::EAGAIN) ||
                raw_err == Some(libc::EWOULDBLOCK)
            { Ok(WriteResult::WouldBlock) }
            else { Err(NetworkError::SocketWriteError(os_err.into())) }
        } else {
            let bytes = result.unwrap();
            if bytes == data.len() as isize { Ok(WriteResult::Sent) }
            else { Ok(WriteResult::Partial(bytes as usize)) }
        }
    }

    pub fn shutdown(&self, shutdown_type: ShutdownType) -> Result<(), NetworkError> {
        syscall!(shutdown(
            self.fd.as_raw_fd(),
            shutdown_type as i32
        )).map_err(|e| NetworkError::SocketShutdownError(e.into()))?;
        Ok(())
    }

    fn _copy(&self, file: &File, mut offset: libc::off_t, count: usize) -> Result<WriteResult, NetworkError> {
        
        let result = syscall!(sendfile(
            self.fd.as_raw_fd(),
            file.as_raw_fd(),
            &mut offset as *mut libc::off_t,
            count
        ));

        if let Err(os_err) = result {
            let raw_err = os_err.raw_os_error();
            if
                raw_err == Some(libc::EAGAIN) ||
                raw_err == Some(libc::EWOULDBLOCK)
            { Ok(WriteResult::WouldBlock) }
            else { Err(NetworkError::SocketWriteError(os_err.into())) }
        } else {
            let bytes = result.unwrap();
            if bytes == count as isize { Ok(WriteResult::Sent) }
            else { Ok(WriteResult::Partial(bytes as usize)) }
        }
    }

    pub fn copy_all(&self, file: &File) -> Result<WriteResult, NetworkError> {
        let len = file.len();
        self._copy(file, 0, len)
    }

    pub fn copy_all_from(&self, file: &File, offset: usize) -> Result<WriteResult, NetworkError> {
        let len = file.len();
        if offset > len {
            return Err(NetworkError::InvalidArgument(
                "Offset is past the end of the file".to_string()
            ));
        }
        self._copy(file, offset as libc::off_t, len - offset)
    }

    pub fn copy_bytes(&self, file: &File, count: usize) -> Result<WriteResult, NetworkError> {
        self._copy(file, 0, count)
    }

    pub fn copy_bytes_from(&self, file: &File, offset: usize, count: usize) -> Result<WriteResult, NetworkError> {
        if offset + count > file.len() {
            return Err(NetworkError::InvalidArgument(
                "Call would read past the end of the file".to_string()
            ));
        }
        self._copy(file, offset as libc::off_t, count)
    }

}

impl SocketConfigurable for TcpStream {
    fn set_opt(&self, option: SocketOption) -> Result<&Self, NetworkError> where Self: Sized {
        option.set(self.as_raw_fd())?;
        Ok(self)
    }
}

impl AsRawFd for TcpStream {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}