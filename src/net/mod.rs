
mod tcp_listener;
mod socket_options;
mod address;
mod tcp_stream;

pub mod futures;

use std::net::AddrParseError;
use thiserror::Error;
use crate::os_error;

pub use tcp_listener::TcpListener;
pub use socket_options::SocketOption;
pub use socket_options::SocketConfigurable;
pub use address::Address;
pub use tcp_stream::TcpStream;


#[derive(Debug, Error, Clone)]
pub enum NetworkError {
    #[error("Failed to create socket: {0}")]
    SocketCreateError(os_error::OSError),

    #[error("Failed to set socket options: {0}")]
    SocketSetOptionError(os_error::OSError),

    #[error("Failed to parse address: {0}")]
    AddressParseError(AddrParseError),

    #[error("Failed to bind socket: {0}")]
    SocketBindError(os_error::OSError),

    #[error("Failed to listen on socket: {0}")]
    SocketListenError(os_error::OSError),

    #[error("Failed to accept connection: {0}")]
    SocketAcceptError(os_error::OSError),

    #[error("Failed to read from socket: {0}")]
    SocketReadError(os_error::OSError),

    #[error("Failed to write to socket: {0}")]
    SocketWriteError(os_error::OSError),

    #[error("Failed to shutdown socket: {0}")]
    SocketShutdownError(os_error::OSError),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}