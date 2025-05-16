

use thiserror::Error;

use crate::OsError;

use super::{tcp_listener::{Listening, TcpListenError, TcpListenerState, WantsBind, WantsListen}, LocalAddress, SocketConfigurable, SocketOption, TcpListener, TcpStream};

pub struct TcpServer<T: TcpListenerState = ()> {
    listeners: Vec<TcpListener<T>>,
    addr: LocalAddress,
}

#[derive(Debug, Error)]
pub enum TcpServerInitError {
    #[error("Invalid worker count: {requested} was given but value must be between 1 and {max}")]
    InvalidWorkerCount {
        requested: usize,
        max: usize,
    },

    #[error("Failed to create listener: {0}")]
    FailedToCreateListener(#[source] OsError),

    #[error("Failed to set socket options on listener: {0}")]
    FailedToSetSocketOptions(#[source] OsError),
}

impl TcpServer<()> {
    pub fn new(addr: LocalAddress) -> Result<TcpServer<WantsBind>, TcpServerInitError> {
        Self::with_listeners(crate::num_threads(), addr)
    }

    pub fn with_listeners(n: usize, addr: LocalAddress) -> Result<TcpServer<WantsBind>, TcpServerInitError> {
        let max_workers = crate::num_threads();
        if n > max_workers {
            return Err(TcpServerInitError::InvalidWorkerCount {
                requested: n,
                max: max_workers,
            });
        }

        let mut listeners = Vec::with_capacity(n);

        for _ in 0..n {

            let listener = TcpListener::new(addr).map_err(|e| {
                TcpServerInitError::FailedToCreateListener(e)
            })?;

            listener.set_opt_multi(&[
                SocketOption::ReuseAddress,
                SocketOption::ReusePort,
            ]).map_err(|e| {
                TcpServerInitError::FailedToSetSocketOptions(e)
            })?;

            listeners.push(listener);
        }

        Ok(TcpServer::<WantsBind> {
            listeners,
            addr,
        })
    }
}

impl TcpServer<WantsBind> {

    pub fn bind(self) -> Result<TcpServer<WantsListen>, OsError> {
        
        let mut listeners = Vec::with_capacity(self.listeners.len());

        for listener in self.listeners {
            listeners.push(listener.bind()?);
        }

        Ok(TcpServer::<WantsListen> {
            listeners,
            addr: self.addr,
        })
    }
}

impl TcpServer<WantsListen> {

    pub fn listen<F, Fut>(
        self,
        handler: F
    ) -> Result<TcpServer<Listening>, TcpListenError> 
    where
        F: Fn(TcpStream) -> Fut + Send + Clone + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut listeners = Vec::with_capacity(self.listeners.len());

        for (i, listener) in self.listeners.into_iter().enumerate() {
            listeners.push(listener.listen_on(i, handler.clone())?);
        }

        Ok(TcpServer::<Listening> {
            listeners,
            addr: self.addr,
        })
    }
}

impl<T: TcpListenerState> SocketConfigurable for TcpServer<T> {
    fn set_opt(&self, option: SocketOption) -> Result<&Self, OsError> {
        for listener in &self.listeners {
            listener.set_opt(option)?;
        }
        Ok(self)
    }
}