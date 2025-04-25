
mod tcp_listener;
mod socket_options;
mod address;
mod tcp_stream;
mod tcp_server;

pub mod futures;

use std::fmt;
use std::net::AddrParseError;
use thiserror::Error;
use crate::os_error;

pub use tcp_listener::TcpListener;
pub use tcp_listener::TcpListenerState;
pub use tcp_listener::WantsBind as TcpListenerStateWantsBind;
pub use tcp_listener::WantsListen as TcpListenerStateWantsListen;
pub use tcp_listener::Listening as TcpListenerStateListening;
pub use socket_options::SocketOption;
pub use socket_options::SocketConfigurable;
pub use address::SocketAddress;
pub use address::IpAddress;
pub use address::IpVersion;
pub use address::PeerAddress;
pub use address::LocalAddress;
pub use address::Port;
pub use tcp_stream::TcpStream;
pub use tcp_stream::IoAcceptFuture;
pub use tcp_server::TcpServer;

#[derive(Debug, Clone, Copy)]
pub struct CSockExtendedError {
    pub errno: u32,
    pub origin: u8,
    pub type_: u8,
    pub code: u8,
    pub pad: u8,
    pub info: u32,
    pub data: u32,
}

impl fmt::Display for CSockExtendedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CSockExtendedError {{ errno: {}, origin: {}, type_: {}, code: {}, pad: {}, info: {}, data: {} }}",
            self.errno, self.origin, self.type_, self.code, self.pad, self.info, self.data
        )
    }
}

impl From<libc::sock_extended_err> for CSockExtendedError {
    fn from(err: libc::sock_extended_err) -> Self {
        Self {
            errno: err.ee_errno,
            origin: err.ee_origin,
            type_: err.ee_type,
            code: err.ee_code,
            pad: err.ee_pad,
            info: err.ee_info,
            data: err.ee_data,
        }
    }
}

impl Into<libc::sock_extended_err> for CSockExtendedError {
    fn into(self) -> libc::sock_extended_err {
        libc::sock_extended_err {
            ee_errno: self.errno,
            ee_origin: self.origin,
            ee_type: self.type_,
            ee_code: self.code,
            ee_pad: self.pad,
            ee_info: self.info,
            ee_data: self.data,
        }
    }
}

#[derive(Debug, Error, Clone)]
pub enum NetworkError {
    #[error("Failed to create socket: {0}")]
    SocketCreateError(os_error::OsError),

    #[error("Failed to set socket options: {0}")]
    SocketSetOptionError(os_error::OsError),

    #[error("Failed to parse address: {0}")]
    AddressParseError(AddrParseError),

    #[error("Unsupported address family: {0}")]
    UnsupportedAddressFamily(libc::sa_family_t),

    #[error("Invalid byte data for address")]
    InvalidAddressByteData,

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("Invalid control message size: {0}")]
    InvalidControlMessageSize(usize),

    #[error("Invalid control message data size, {0} is not a multiple of {1}")]
    InvalidControlMessageDataSize(usize, usize),

    #[error("Failed to bind socket: {0}")]
    SocketBindError(os_error::OsError),

    #[error("Invalid control message type for level {0} and type {1}")]
    InvalidControlMessageTypeForLevel(i32, i32),

    #[error("Invalid control message level: {0}")]
    InvalidControlMessageLevel(i32),

    #[error("Failed to listen on socket: {0}")]
    SocketListenError(os_error::OsError),

    #[error("Failed to accept connection: {0}")]
    SocketAcceptError(os_error::OsError),

    #[error("Failed to read from socket: {0}")]
    SocketReadError(os_error::OsError),

    #[error("Failed to write to socket: {0}")]
    SocketWriteError(os_error::OsError),

    #[error("Failed to shutdown socket: {0}")]
    SocketShutdownError(os_error::OsError),

    #[error("Failed to get socket name: {0}")]
    SocketGetNameError(os_error::OsError),

    #[error("Socket failed to connect to host: {addr}, {message}")]
    SocketConnectError {
        addr: SocketAddress,
        message: String,
    },

    #[error("Failed to start listener task: {0}")]
    ListenerTaskError(String),

    #[error("Extended socket error: {0}")]
    ExtendedSocketError(CSockExtendedError),

    #[error("Null pointer")]
    NullPointerError,

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
}