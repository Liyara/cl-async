use std::{os::fd::{AsRawFd, OwnedFd}, pin::Pin, sync::Arc, task::{Context, Poll, Waker}};

use crate::worker::state;

use super::{NetworkError, TcpStream};

#[derive(Debug, Clone)]
enum State {
    None,
    Waiting,
    Complete(TcpStream),
    Error(NetworkError),
}

pub struct AcceptFuture {
    fd: Arc<OwnedFd>,
    state: Arc<State>,
}

impl AcceptFuture {

    pub fn new(fd: Arc<OwnedFd>) -> Self {
        Self {
            fd,
            state: Arc::new(State::None),
        }
    }

    /*fn register_fd(waker: &Waker) {

    }

    fn try_accept(&self, waker: &Waker) -> Result<Option<TcpStream>, NetworkError> {

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
            {
                register_fd(waker);
                Ok(None)
            }
            else { Err(NetworkError::SocketAcceptError(os_err.into())) }
        } else { Ok(Some(TcpStream::new(result.unwrap(), addr))) }
    }*/
}

/*impl Future for AcceptFuture {
    type Output = Result<TcpStream, NetworkError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {

        let this = self.as_mut().get_mut();
        let state = this.state.clone();

        match state {
            State::None => {
                let result = this.try_accept(cx.waker());
                match result {
                    Ok(Some(tcp_stream)) => {
                        Poll::Ready(Ok(tcp_stream))
                    },
                    Ok(None) => {
                        *this.state = State::Waiting;
                        Poll::Pending
                    },
                    Err(e) => {
                        Poll::Ready(Err(e))
                    }
                }
            },
            // Spurious wakeup, do nothing
            State::Waiting => Poll::Pending,
            State::Complete(tcp_stream) => {
                Poll::Ready(Ok(tcp_stream))
            },
            State::Error(e) => {
                Poll::Ready(Err(e))
            }
        }    
    }
}*/