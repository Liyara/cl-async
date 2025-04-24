

use super::{LocalAddress, NetworkError, SocketConfigurable, SocketOption, TcpListener, TcpStream};


pub struct TcpServer {
    listeners: Vec<TcpListener>,
}

impl TcpServer {

    pub fn new() -> Result<Self, NetworkError> {
        Self::with_listeners(crate::num_threads())
    }

    pub fn num_listeners(&self) -> usize {
        self.listeners.len()
    }

    pub fn with_listeners(n: usize) -> Result<Self, NetworkError> {

        if n > crate::num_threads() {
            return Err(NetworkError::InvalidArgument(String::from(
                "Number of listeners  cannot exceed number of worker threads"
            )));
        }

        let mut listeners = Vec::with_capacity(n);

        for _ in 0..n {
            let listener = TcpListener::new()?;
            listener.set_opt_multi(&[
                SocketOption::ReuseAddress,
                SocketOption::ReusePort,
            ])?;
            listeners.push(listener);
        }

        Ok(Self { listeners })
    }

    pub fn bind(
        mut self,
        address: LocalAddress
    ) -> Result<Self, NetworkError> {
        for _ in 0..self.listeners.len() {
            let listener = self.listeners.pop().ok_or(
                NetworkError::InvalidArgument(String::from(
                    "No listeners available"
                ))
            )?;

            self.listeners.push(listener.bind(address.clone())?);
        }
        Ok(self)
    }

    pub fn listen<F, Fut>(
        mut self,
        backlog: i32,
        handler: F
    ) -> Result<Self, NetworkError> 
    where
        F: Fn(TcpStream) -> Fut + Send + Clone + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        for i in 0..self.listeners.len() {

            let listener = self.listeners.pop().ok_or(
                NetworkError::InvalidArgument(String::from(
                    "No listeners available"
                ))
            )?;

            self.listeners.push(listener.listen_on(
                i,
                backlog,
                handler.clone()
            )?);
        }
        Ok(self)
    }

    
}

impl SocketConfigurable for TcpServer {
    fn set_opt(&self, option: SocketOption) -> Result<&Self, NetworkError> {
        for listener in &self.listeners {
            listener.set_opt(option)?;
        }
        Ok(self)
    }
}
    