

use super::{tcp_listener::{Listening, TcpListenerState, WantsBind, WantsListen}, LocalAddress, NetworkError, SocketConfigurable, SocketOption, TcpListener, TcpStream};

pub struct TcpServer<T: TcpListenerState = ()> {
    listeners: Vec<TcpListener<T>>,
    addr: LocalAddress,
}

impl TcpServer<()> {
    pub fn new(addr: LocalAddress) -> Result<TcpServer<WantsBind>, NetworkError> {
        Self::with_listeners(crate::num_threads(), addr)
    }

    pub fn with_listeners(n: usize, addr: LocalAddress) -> Result<TcpServer<WantsBind>, NetworkError> {
        if n > crate::num_threads() {
            return Err(NetworkError::InvalidArgument(String::from(
                "Number of listeners cannot exceed number of worker threads"
            )));
        }

        let mut listeners = Vec::with_capacity(n);

        for _ in 0..n {
            let listener = TcpListener::new(addr)?;
            listener.set_opt_multi(&[
                SocketOption::ReuseAddress,
                SocketOption::ReusePort,
            ])?;
            listeners.push(listener);
        }

        Ok(TcpServer::<WantsBind> {
            listeners,
            addr,
        })
    }
}

impl TcpServer<WantsBind> {

    pub fn bind(self) -> Result<TcpServer<WantsListen>, NetworkError> {
        
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
    ) -> Result<TcpServer<Listening>, NetworkError> 
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
    fn set_opt(&self, option: SocketOption) -> Result<&Self, NetworkError> {
        for listener in &self.listeners {
            listener.set_opt(option)?;
        }
        Ok(self)
    }
}