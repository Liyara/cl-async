use std::os::fd::{
    AsRawFd, 
    FromRawFd, 
    RawFd
};

use crate::io::{
    operation::future::{
        IoOperationFuture, IoVoidFuture, __async_impl_copyable__, __async_impl_receiver__, __async_impl_sender__, __async_impl_types__
    }, IoError, IoOperation, IoResult, OwnedFdAsync
};

use super::{
    LocalAddress, NetworkError, PeerAddress, SocketConfigurable, SocketOption
};

pub struct TcpStream {
    fd: OwnedFdAsync,
    local_addr: Option<LocalAddress>,
    peer_addr: Option<PeerAddress>
}

impl TcpStream {

    pub (super) fn new(
        fd: RawFd, 
        local_addr: Option<LocalAddress>,
        peer_addr: Option<PeerAddress>
    ) -> Self {
        Self {
            fd: unsafe { OwnedFdAsync::from_raw_fd(fd) },
            local_addr,
            peer_addr
        }
    }

    async fn connect_client(
        fd: &OwnedFdAsync,
        addr: &PeerAddress
    ) -> Result<(), IoError> {

        IoVoidFuture::new(
            IoOperation::connect(fd, addr.clone())?
        ).await?;

        Ok(())
    }

    pub async fn new_client(peer_addr: PeerAddress) -> Result<Self, NetworkError> {

        let fd = syscall!(socket(
            libc::AF_INET,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0
        )).map_err(|e| {
            NetworkError::SocketCreateError(e.into())
        })?;

        let fd = unsafe { OwnedFdAsync::from_raw_fd(fd) };

        Self::connect_client(&fd, &peer_addr).await.map_err(
            |e| {
                NetworkError::SocketConnectError {
                    addr: peer_addr.clone().into_socket_address(),
                    message: e.to_string()
                }
            }
        )?;

        Ok(Self {
            fd,
            local_addr: None,
            peer_addr: Some(peer_addr)
        })
    }

    pub fn query_local_address(&self) -> Result<LocalAddress, NetworkError> {
        
        Ok(
            match self.local_addr {
                Some(ref addr) => addr.clone(),
                None => LocalAddress::try_from(&self.as_raw_fd())?
            }
        )
    }

    pub fn query_peer_address(&self) -> Result<PeerAddress, NetworkError> {
        
        Ok(
            match self.peer_addr {
                Some(ref addr) => addr.clone(),
                None => PeerAddress::try_from(&self.as_raw_fd())?
            }
        )
    }

    pub fn address_local(&mut self) -> Result<&LocalAddress, NetworkError> {
        
        if self.local_addr.is_none() {
            self.local_addr = Some(LocalAddress::try_from(&self.as_raw_fd())?);
        }

        Ok(self.local_addr.as_ref().unwrap())
    }

    pub fn address_peer(&mut self) -> Result<&PeerAddress, NetworkError> {
        
        if self.peer_addr.is_none() {
            self.peer_addr = Some(PeerAddress::try_from(&self.as_raw_fd())?);
        }

        Ok(self.peer_addr.as_ref().unwrap())
    }

    pub async fn shutdown(&self) -> IoResult<()> {
        IoVoidFuture::new(
            IoOperation::shutdown(self)
        ).await
    }

    pub async fn shutdown_read(&self) -> IoResult<()> {
        IoVoidFuture::new(
            IoOperation::shutdown_read(self)
        ).await
    }

    pub async fn shutdown_write(&self) -> IoResult<()> {
        IoVoidFuture::new(
            IoOperation::shutdown_write(self)
        ).await
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

__async_impl_types__!(TcpStream);
__async_impl_receiver__!(TcpStream);
__async_impl_sender__!(TcpStream);
__async_impl_copyable__!(TcpStream);

pub type IoAcceptFuture = IoOperationFuture<crate::net::TcpStream>;