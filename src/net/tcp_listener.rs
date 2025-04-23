use std::{os::fd::{AsRawFd, FromRawFd, RawFd}, sync::Arc};

use futures::FutureExt;

use crate::{io::{IoCompletion, IoOperation, OwnedFdAsync}, notifications::NotificationFlags, worker::WorkerHandle, Key};

use super::{IpAddress, NetworkError, SocketAddress, SocketConfigurable, SocketOption, TcpStream};

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

    pub fn bind(self, addr: SocketAddress) -> Result<Self, NetworkError> {
        let addr_len = match addr.ip() {
            IpAddress::V4(_) => std::mem::size_of::<libc::sockaddr_in>(),
            IpAddress::V6(_) => std::mem::size_of::<libc::sockaddr_in6>(),
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

    pub fn listen_on<F, Fut>(
        self,
        worker: usize,
        backlog: i32,
        handler: F
    ) -> Result<Self, NetworkError>
    where
        F: Fn(TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let fd = self.as_raw_fd();

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
                    _ = notification_subscription.recv().fuse() => {
                        info!("cl-async: Shutting down listener");
                        break;
                    }
                }
                
            }
        }).map_err(|e| {
            NetworkError::ListenerTaskError(e.to_string())
        })?;

        Ok(self)
    }

    pub fn listen<F, Fut>(
        self, 
        backlog: i32,
        handler: F
    ) -> Result<Self, NetworkError> 
    where
        F: Fn(TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    { self.listen_on(crate::next_worker_id(), backlog, handler) }

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