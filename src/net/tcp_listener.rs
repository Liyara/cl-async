use std::{os::fd::{AsRawFd, FromRawFd, RawFd}, sync::Arc};

use futures::FutureExt;
use sysctl::Sysctl;

use crate::{io::{IoCompletion, IoOperation, OwnedFdAsync}, notifications::NotificationFlags, worker::WorkerHandle, Key};

use super::{IpVersion, LocalAddress, NetworkError, SocketConfigurable, SocketOption, TcpStream};

#[derive(Debug, Clone)]
pub struct TcpListener {
    fd: Arc<OwnedFdAsync>,
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
            fd: unsafe { Arc::new(OwnedFdAsync::from_raw_fd(fd)) }
        })
    }

    pub fn bind(self, addr: LocalAddress) -> Result<Self, NetworkError> {
        let addr_len = match addr.ip().version() {
            IpVersion::V4 => std::mem::size_of::<libc::sockaddr_in>(),
            IpVersion::V6 => std::mem::size_of::<libc::sockaddr_in6>(),
        };
        let addr: libc::sockaddr_storage = addr.try_into()?;

        syscall!(bind(
            self.fd.as_raw_fd(),
            &addr as *const _ as *const libc::sockaddr,
            addr_len as u32
        )).map_err(|e| {
            NetworkError::SocketBindError(e.into())
        })?;

        Ok(self)
    }

    fn get_worker_data(
        &self,
        id: usize,
    ) -> Result<(WorkerHandle, Key, crate::notifications::Subscription), NetworkError> {
        let worker_handle = crate::get_worker_handle(id).map_err(
            |e| NetworkError::ListenerTaskError(e.to_string())
        )?;

        let key = worker_handle.io_submission_queue.submit(
            IoOperation::accept_multi(&self.fd.as_raw_fd()),
            None
        ).map_err(|e| {
            NetworkError::ListenerTaskError(e.to_string())
        })?;

        let notification_subscription = worker_handle.notify_on(
            NotificationFlags::SHUTDOWN | NotificationFlags::KILL
        ).map_err(|e| {
            NetworkError::ListenerTaskError(e.to_string())
        })?;

        Ok((worker_handle, key, notification_subscription))
    }

    fn try_get_backlog() -> Result<i32, ()> {
        let ctl = sysctl::Ctl::new("net.core.somaxconn").map_err(|_| ())?;
        let backlog_str = ctl.value_string().map_err(|_| ())?;
        backlog_str.parse::<i32>().map_err(|_| ())
    }

    pub fn listen_on<F, Fut>(
        self,
        worker: usize,
        handler: F
    ) -> Result<Self, NetworkError>
    where
        F: Fn(TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let fd = self.as_raw_fd();

        let backlog = match Self::try_get_backlog() {
            Ok(backlog) => backlog,
            Err(_) => 4096
        };

        syscall!(listen(
            fd,
            backlog
        )).map_err(|e| {
            NetworkError::SocketListenError(e.into())
        })?;

        let (
            worker_handle, 
            key, 
            mut notification_subscription
        ) = self.get_worker_data(worker)?;

        crate::spawn(async move {
            
            loop {

                futures::select! {
                    result = std::future::poll_fn(|cx| {
                        worker_handle.io_completion_queue.poll(key, cx)
                    }).fuse() => {
                        let mut stream = match result {
                            Some(Ok(IoCompletion::Accept(completion))) => {
                                TcpStream::new(
                                    completion.fd,
                                    None,
                                    completion.address
                                )
                            },
                            Some(Err(e)) => {
                                error!("cl-aysnc: Failed to accept connection: {e}");
                                continue;
                            },
                            _ => continue
                        };
        
                        info!("cl-async: Accepted connection from {}", stream.address_peer().unwrap());
        
                        if let Err(e) = crate::spawn(handler(stream)) {
                            error!("cl-async: Failed to spawn handler task: {e}");
                            continue;
                        }
                    },
                    _ = notification_subscription.recv().fuse() => { break; }
                }
                
            }
            info!("cl-async: Shutting down listener");
        }).map_err(|e| {
            NetworkError::ListenerTaskError(e.to_string())
        })?;

        Ok(self)
    }

    pub fn listen<F, Fut>(
        self, 
        handler: F
    ) -> Result<Self, NetworkError> 
    where
        F: Fn(TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    { self.listen_on(crate::next_worker_id(), handler) }

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