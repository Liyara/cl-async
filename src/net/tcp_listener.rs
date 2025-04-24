use std::{os::fd::{AsRawFd, FromRawFd, RawFd}, sync::Arc};

use futures::FutureExt;
use sysctl::Sysctl;

use crate::{io::{IoCompletion, IoOperation, OwnedFdAsync}, notifications::NotificationFlags, worker::WorkerHandle, Key};

use super::{IpVersion, LocalAddress, NetworkError, SocketConfigurable, SocketOption, TcpStream};

pub trait TcpListenerState {}

pub struct WantsBind {
    _priv: (),
}
impl TcpListenerState for WantsBind {}

pub struct WantsListen{
    _priv: (),
}
impl TcpListenerState for WantsListen {}

pub struct Listening{
    _priv: (),
}
impl TcpListenerState for Listening {}

impl TcpListenerState for () {}

#[derive(Debug, Clone)]
pub struct TcpListener<T: TcpListenerState = ()> {
    fd: Arc<OwnedFdAsync>,
    addr: LocalAddress,
    _state: std::marker::PhantomData<T>,
}

impl<T: TcpListenerState> TcpListener<T> {
    fn transition_state<U: TcpListenerState>(self) -> TcpListener<U> {
        TcpListener::<U> {
            fd: self.fd,
            addr: self.addr,
            _state: std::marker::PhantomData::<U>,
        }
    }
}

impl TcpListener<()> {

    pub fn new(addr: LocalAddress) -> Result<TcpListener<WantsBind>, NetworkError> {

        let domain = match addr.ip().version() {
            IpVersion::V4 => libc::AF_INET,
            IpVersion::V6 => libc::AF_INET6,
        };

        let fd = syscall!(socket(
            domain,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0
        )).map_err(|e| {
            NetworkError::SocketCreateError(e.into())
        })?;

        Ok(TcpListener::<WantsBind> {
            fd: unsafe { Arc::new(OwnedFdAsync::from_raw_fd(fd)) },
            addr,
            _state: std::marker::PhantomData::<WantsBind>,
        })
    }

}

impl TcpListener<WantsBind> {
    pub fn bind(self) -> Result<TcpListener<WantsListen>, NetworkError> {

        let addr_len = match self.addr.ip().version() {
            IpVersion::V4 => std::mem::size_of::<libc::sockaddr_in>(),
            IpVersion::V6 => std::mem::size_of::<libc::sockaddr_in6>(),
        };

        let addr: libc::sockaddr_storage = self.addr.try_into()?;

        syscall!(bind(
            self.as_raw_fd(),
            &addr as *const _ as *const libc::sockaddr,
            addr_len as u32
        )).map_err(|e| {
            NetworkError::SocketBindError(e.into())
        })?;

        Ok(Self::transition_state::<WantsListen>(self))
    }
}

impl TcpListener<WantsListen> {

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
    ) -> Result<TcpListener<Listening>, NetworkError>
    where
        F: Fn(TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let fd = Arc::clone(&self.fd);

        let backlog = match Self::try_get_backlog() {
            Ok(backlog) => backlog,
            Err(_) => 4096
        };

        syscall!(listen(
            fd.as_raw_fd(),
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
            drop(fd);
            
        }).map_err(|e| {
            NetworkError::ListenerTaskError(e.to_string())
        })?;

        Ok(Self::transition_state::<Listening>(self))
    }

    pub fn listen<F, Fut>(
        self, 
        handler: F
    ) -> Result<TcpListener<Listening>, NetworkError> 
    where
        F: Fn(TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    { self.listen_on(crate::next_worker_id(), handler) }
}

impl<T: TcpListenerState> AsRawFd for TcpListener<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl<T: TcpListenerState> SocketConfigurable for TcpListener<T> {
    fn set_opt(&self, option: SocketOption) -> Result<&Self, NetworkError> {
        option.set(self.fd.as_raw_fd())?;
        Ok(self)
    }
}