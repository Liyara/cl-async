use std::os::fd::RawFd;
use crate::OsError;

macro_rules! impl_option {

    ($name:ident, $level:ident, $option:ident) => {
        fn $name<T>(fd: RawFd, value: T) -> Result<(), OsError> {
            unsafe {
                Self::setsockopt(fd, libc::$level, libc::$option, value)
            }
        }
    };
}

static TRUE: libc::c_int = 1;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Linger {
    pub onoff: bool,
    pub linger: i32,
}

impl Linger {

    fn as_c_linger(&self) -> libc::linger {
        libc::linger {
            l_onoff: self.onoff as i32,
            l_linger: self.linger,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum SocketOption {
    Debug,
    Broadcast,
    ReuseAddress,
    ReusePort,
    KeepAlive,
    NoDelay,
    Linger(Linger),
    OutOfBandInline,
    SendBuffer(u32),
    ReceiveBuffer(u32),
    DontRoute,
}

impl SocketOption {

    unsafe fn setsockopt<T>(fd: RawFd, level: i32, option: i32, value: T) -> Result<(), OsError> {
        syscall!(
            setsockopt(
                fd,
                level,
                option,
                &value as *const _ as *const libc::c_void,
                std::mem::size_of_val(&value) as libc::socklen_t
            )
        ).map_err(OsError::from)?;

        Ok(())
    }

    impl_option!(debug,                 SOL_SOCKET,     SO_DEBUG        );
    impl_option!(broadcast,             SOL_SOCKET,     SO_BROADCAST    );
    impl_option!(reuse_address,         SOL_SOCKET,     SO_REUSEADDR    );
    impl_option!(reuse_port,            SOL_SOCKET,     SO_REUSEPORT    );
    impl_option!(keepalive,             SOL_SOCKET,     SO_KEEPALIVE    );
    impl_option!(no_delay,              IPPROTO_TCP,    TCP_NODELAY     );
    impl_option!(linger,                SOL_SOCKET,     SO_LINGER       );
    impl_option!(out_of_band_inline,    SOL_SOCKET,     SO_OOBINLINE    );
    impl_option!(send_buffer,           SOL_SOCKET,     SO_SNDBUF       );
    impl_option!(receive_buffer,        SOL_SOCKET,     SO_RCVBUF       );
    impl_option!(dont_route,            SOL_SOCKET,     SO_DONTROUTE    );

    pub fn set(&self, fd: RawFd) -> Result<(), OsError> {
        match self {
            Self::Debug => Self::debug(fd, TRUE),
            Self::Broadcast => Self::broadcast(fd, TRUE),
            Self::ReuseAddress => Self::reuse_address(fd, TRUE),
            Self::ReusePort => Self::reuse_port(fd, TRUE),
            Self::KeepAlive => Self::keepalive(fd, TRUE),
            Self::NoDelay => Self::no_delay(fd, TRUE),
            Self::Linger(linger) => Self::linger(fd, linger.as_c_linger()),
            Self::OutOfBandInline => Self::out_of_band_inline(fd, TRUE),
            Self::SendBuffer(size) => Self::send_buffer(fd, *size as libc::c_int),
            Self::ReceiveBuffer(size) => Self::receive_buffer(fd, *size as libc::c_int),
            Self::DontRoute => Self::dont_route(fd, TRUE),
        }
    }
}

pub trait SocketConfigurable {
    fn set_opt(&self, option: SocketOption) -> Result<&Self, OsError> where Self: Sized;
    fn set_opt_multi(&self, options: &[SocketOption]) -> Result<&Self, OsError> where Self: Sized {
        for option in options {
            self.set_opt(*option)?;
        }
        Ok(self)
    }
}