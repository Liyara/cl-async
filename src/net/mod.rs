
mod tcp_listener;
mod socket_options;
mod address;
mod tcp_stream;
mod tcp_server;

pub mod futures;
use thiserror::Error;

pub use tcp_listener::TcpListener;
pub use tcp_listener::TcpListenerState;
pub use tcp_listener::WantsBind as TcpListenerStateWantsBind;
pub use tcp_listener::WantsListen as TcpListenerStateWantsListen;
pub use tcp_listener::Listening as TcpListenerStateListening;
pub use tcp_listener::TcpListenError;
pub use socket_options::SocketOption;
pub use socket_options::SocketConfigurable;
pub use address::SocketAddress;
pub use address::IpAddress;
pub use address::IpVersion;
pub use address::PeerAddress;
pub use address::LocalAddress;
pub use address::Port;
pub use address::AddressRetrievalError;
pub use address::IpParseError;
pub use address::IpV4ParseError;
pub use address::IpV6CompressError;
pub use address::IpV6UncompressError;
pub use address::UnsupportedAddressFamilyError;
pub use address::V4MappedV6Error;
pub use address::IpError;
pub use tcp_stream::TcpStream;
pub use tcp_stream::IoAcceptFuture;
pub use tcp_stream::TcpConnectionError;
pub use tcp_server::TcpServer;
pub use tcp_server::TcpServerInitError;

pub type ListeningTcpListener = TcpListener<TcpListenerStateListening>;
pub type ListeningTcpServer = TcpServer<ListeningTcpListener>;

#[derive(Debug, Clone, Copy, Error)]
#[error("C Socket Extended Error: {errno}, origin: {origin}, type: {type_}, code: {code}, pad: {pad}, info: {info}, data: {data}")]
pub struct CSockExtendedError {
    pub errno: u32,
    pub origin: u8,
    pub type_: u8,
    pub code: u8,
    pub pad: u8,
    pub info: u32,
    pub data: u32,
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

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("IP error: {0}")]
    IpError(#[from] IpError),

    #[error("TCP Listen error: {0}")]
    TcpListenError(#[from] TcpListenError),

    #[error("TCP Server Initialization error: {0}")]
    TcpServerInitError(#[from] TcpServerInitError),

    #[error("TCP Connection error: {0}")]
    TcpConnectionError(#[from] TcpConnectionError),
}