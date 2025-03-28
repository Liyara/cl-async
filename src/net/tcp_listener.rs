use std::{os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd}, sync::Arc};

use super::{futures::AcceptFuture, Address, NetworkError, SocketConfigurable, SocketOption, TcpStream};


pub struct TcpListener {
    fd: Arc<OwnedFd>,
}

impl TcpListener {

    pub fn new() -> Result<Self, NetworkError> {

        let fd = syscall!(socket(
            libc::AF_INET,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0
        )).map_err(|e| {
            NetworkError::SocketCreateError(e.into())
        })?;

        Ok(Self {
            fd: unsafe { Arc::new(OwnedFd::from_raw_fd(fd)) }
        })
    }

    pub fn bind(&self, addr: Address) -> Result<&Self, NetworkError> {
        let addr: libc::sockaddr_in = addr.try_into()?;

        syscall!(bind(
            self.fd.as_raw_fd(),
            &addr as *const libc::sockaddr_in as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_in>() as u32
        )).map_err(|e| {
            NetworkError::SocketBindError(e.into())
        })?;

        Ok(self)
    }

    pub fn listen(&self, backlog: i32) -> Result<(), NetworkError> {
        syscall!(listen(
            self.fd.as_raw_fd(),
            backlog
        )).map_err(|e| {
            NetworkError::SocketListenError(e.into())
        })?;

        Ok(())
    }

    pub fn accept(&self) -> AcceptFuture {
        AcceptFuture::new(Arc::clone(&self.fd))
    }

    pub fn try_accept(&self) -> Result<Option<TcpStream>, NetworkError> {

        let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
        let mut addr_len = std::mem::size_of::<libc::sockaddr_in>() as u32;

        let result = syscall!(accept4(
            self.fd.as_raw_fd(),
            &mut addr as *mut libc::sockaddr_in as *mut libc::sockaddr,
            &mut addr_len as *mut u32,
            libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC
        ));

        if let Err(os_err) = result {
            let raw_err = os_err.raw_os_error();
            if 
                raw_err == Some(libc::EWOULDBLOCK) ||
                raw_err == Some(libc::EAGAIN)
            { Ok(None) }
            else { Err(NetworkError::SocketAcceptError(os_err.into())) }
        } else { Ok(Some(TcpStream::new(result.unwrap(), addr))) }
    }

}

impl AsRawFd for TcpListener {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl SocketConfigurable for TcpListener {
    fn set_opt(&self, option: SocketOption) -> Result<&Self, NetworkError> {
        option.set(self.fd.as_raw_fd())?;
        Ok(self)
    }
}