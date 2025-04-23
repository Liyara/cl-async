use std::{fmt, os::fd::RawFd};

use super::NetworkError;

pub enum IpVersion {
    V4,
    V6,
}

// Byte data is stored as big-endian
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum IpAddress {
    V4([u8; 4]),
    V6([u8; 16]),
}

impl IpAddress {

    pub fn v4(addr: &str) -> Result<Self, NetworkError> {
        Self::try_from(addr.to_string())
    }

    pub fn v6(addr: &str) -> Result<Self, NetworkError> {
        Self::try_from(addr.to_string())
    }

    pub fn to_be_bytes(&self) -> [u8; 16] {
        match self {
            IpAddress::V4(addr) => {
                let mut bytes = [0u8; 16];
                bytes[10] = 0xff;
                bytes[11] = 0xff;
                bytes[12..].copy_from_slice(addr);
                bytes
            }
            IpAddress::V6(addr) => *addr,
        }
    }

    pub fn version(&self) -> IpVersion {
        match self {
            IpAddress::V4(_) => IpVersion::V4,
            IpAddress::V6(_) => IpVersion::V6,
        }
    }

    fn v4_from_str(addr: &str) -> Result<Self, NetworkError> {
        let parts: Vec<&str> = str::split(addr, '.').collect();

        if parts.len() != 4 {
            return Err(NetworkError::InvalidAddress(addr.to_string()));
        }

        let mut bytes = [0u8; 4];

        for i in 0..4 {
            let part = parts[i].parse::<u8>().map_err(|_| {
                NetworkError::InvalidAddress(addr.to_string())
            })?;

            bytes[i] = part;
        }

        Ok(IpAddress::V4(bytes))
    }

    fn compress_v6(addr: &str) -> Result<String, NetworkError> {

        let mut parts: Vec<String> = str::split(
            addr, 
            ':'
        ).map(|s| s.trim().to_string())
        .collect();

        if parts.len() != 8 {
            return Err(NetworkError::InvalidArgument(addr.to_string()));
        }

        for part in parts.iter_mut() {

            if part.len() == 0 || part.len() > 4 {
                return Err(NetworkError::InvalidArgument(addr.to_string()));
            }

            while part.len() > 1 && part.starts_with("0") {
                part.remove(0);
            }
        }

        let mut double_colon_index: isize = -1;
        let mut zero_count = 0;
        let mut most_zeroes = 0;
        let mut i = 0;

        for part in parts.iter_mut() {

            if !part.is_empty() {
                if part.len() == 1 && part.starts_with("0") {
                    zero_count += 1;
                    if zero_count > most_zeroes {
                        most_zeroes = zero_count;
                        double_colon_index = (i + 1) - zero_count;
                    }
                } else {
                    zero_count = 0;
                }
            } else {
                return Err(NetworkError::InvalidArgument(addr.to_string()));
            }
            i += 1;
        }

        if double_colon_index > -1 && most_zeroes > 1 {
            let mut remaining = most_zeroes;
            while remaining > 1 {
                parts.remove(double_colon_index as usize);
                remaining -= 1;
            }

            if remaining == 1 {
                parts[double_colon_index as usize].clear();
            }
        }

        if parts.len() == 1 && parts[0].is_empty() {
            return Ok(String::from("::"));
        }

        let recombined = parts.join(":");
        Ok(recombined)
        
    }

    fn uncompress_v6(addr: &str) -> Result<String, NetworkError> {
        let mut parts: Vec<String> = str::split(
            addr, 
            ':'
        ).map(|s| s.trim().to_string())
        .collect();

        if parts.len() > 9 || parts.len() < 3 {
            return Err(NetworkError::InvalidArgument(addr.to_string()));
        }

        if parts[0].is_empty() { parts.remove(0);}

        if let Some(part) = parts.last() {
            if part.is_empty() {
                parts.pop();
            }
        }

        let mut double_colon_index = -1;

        for i in 0..parts.len() {

            let part = &mut parts[i];

            if part.len() > 4 {
                return Err(NetworkError::InvalidArgument(addr.to_string()));
            }

            if part.is_empty() {
                if double_colon_index == -1 {
                    double_colon_index = i as isize;
                    continue;
                } else {
                    return Err(NetworkError::InvalidArgument(addr.to_string()));
                }
            }

            while part.len() < 4 {
                part.insert(0, '0');
            }
        }

        if double_colon_index == -1 {
            if parts.len() != 8 {
                return Err(NetworkError::InvalidArgument(addr.to_string()))
            } else {
                return Ok(parts.join(":"))
            }
        }

        parts[double_colon_index as usize] = String::from("0000");
        while parts.len() < 8 {
            parts.insert(double_colon_index as usize, String::from("0000"));
        }

        let recombined = parts.join(":");
        Ok(recombined)
    }

    fn uncompressed_v6_as_bytes(addr: &str) -> Result<[u8; 16], NetworkError> {
        let mut bytes = [0u8; 16];
        let parts: Vec<&str> = str::split(addr, ':').collect();

        if parts.len() != 8 {
            return Err(NetworkError::InvalidAddress(addr.to_string()));
        }

        for i in 0..8 {
            let part = u16::from_str_radix(parts[i], 16).map_err(|_| {
                NetworkError::InvalidAddress(addr.to_string())
            })?;

            bytes[i * 2] = (part >> 8) as u8;
            bytes[i * 2 + 1] = part as u8;
        }

        Ok(bytes)
    }

    fn bytes_as_uncompressed_v6(bytes: &[u8; 16]) -> String {
        let mut parts = Vec::new();

        for i in 0..8 {
            let part = u16::from_be_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
            parts.push(format!("{:04x}", part));
        }

        parts.join(":")
    }

}

impl TryFrom<String> for IpAddress {
    type Error = NetworkError;
    fn try_from(ip: String) -> Result<Self, Self::Error> {
        if let Ok(addr) = Self::v4_from_str(&ip) {
            Ok(addr)
        }
        else if let Ok(addr) = Self::uncompress_v6(&ip) {
            Ok(Self::V6(Self::uncompressed_v6_as_bytes(&addr)?))
        } else {
            Err(NetworkError::InvalidAddress(ip))
        }
    }
}

impl fmt::Display for IpAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            IpAddress::V4(addr) => {
                write!(f, "{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])
            }
            IpAddress::V6(addr) => {
                let uncompressed = Self::bytes_as_uncompressed_v6(addr);
                let compressed = Self::compress_v6(&uncompressed).unwrap_or_else(|_| uncompressed.clone());
                write!(f, "{}", compressed)
            }
        }
    }
}


#[derive(Debug, Clone)]
pub struct SocketAddress {
    ip: IpAddress,
    port: u16,
}

impl SocketAddress {

    pub fn ip(&self) -> &IpAddress { &self.ip }
    pub fn port(&self) -> u16 { self.port }

    pub fn new(ip: IpAddress, port: u16) -> Self {
        Self { ip, port }
    }
}

impl TryInto<libc::sockaddr_storage> for SocketAddress {
    type Error = NetworkError;
    fn try_into(self) -> Result<libc::sockaddr_storage, NetworkError> {
        match self.ip {
            IpAddress::V4(be_bytes) => {

                let addr_in = libc::sockaddr_in {
                    sin_family: libc::AF_INET as u16,
                    sin_port: self.port.to_be(),
                    sin_addr: libc::in_addr {
                        s_addr: u32::from_be_bytes(be_bytes),
                    },
                    sin_zero: [0; 8],
                };

                let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };

                unsafe {
                    let storage_ptr = &mut storage as *mut _ as *mut libc::sockaddr_in;
                    std::ptr::write(storage_ptr, addr_in);
                }

                Ok(storage)
            },
            IpAddress::V6(be_bytes) => {

                let addr_in6 = libc::sockaddr_in6 {
                    sin6_family: libc::AF_INET6 as u16,
                    sin6_port: self.port.to_be(),
                    sin6_addr: libc::in6_addr {
                        s6_addr: be_bytes,
                    },
                    sin6_flowinfo: 0,
                    sin6_scope_id: 0,
                };

                let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };

                unsafe {
                    let storage_ptr = &mut storage as *mut _ as *mut libc::sockaddr_in6;
                    std::ptr::write(storage_ptr, addr_in6);
                }

                Ok(storage)
            }
        }
    }
}


impl TryFrom<libc::sockaddr_storage> for SocketAddress {
    type Error = NetworkError;
    fn try_from(addr: libc::sockaddr_storage) -> Result<Self, NetworkError> {
        let version = addr.ss_family;

        if version == libc::AF_INET as libc::sa_family_t {
            let addr_in = unsafe {
                *(&addr as *const libc::sockaddr_storage as *const libc::sockaddr_in)
            };


            Ok(Self {
                ip: IpAddress::V4(u32::from_be(addr_in.sin_addr.s_addr).to_be_bytes()),
                port: u16::from_be(addr_in.sin_port),
            })
        } else if version == libc::AF_INET6 as libc::sa_family_t {
            let addr_in6 = unsafe {
                *(&addr as *const libc::sockaddr_storage as *const libc::sockaddr_in6)
            };

            Ok(Self {
                ip: IpAddress::V6(addr_in6.sin6_addr.s6_addr),
                port: u16::from_be(addr_in6.sin6_port),
            })
        } else {
            Err(NetworkError::UnsupportedAddressFamily(version))
        }
    }
}

impl std::fmt::Display for SocketAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

#[derive(Debug, Clone)]
pub struct PeerAddress(SocketAddress);

impl PeerAddress {

    pub fn ip(&self) -> &IpAddress { self.0.ip() }
    pub fn port(&self) -> u16 { self.0.port() }

    pub fn new(socket_address: SocketAddress) -> Self {
        Self(socket_address)
    }

    pub fn as_socket_address(&self) -> &SocketAddress {
        &self.0
    }

    pub fn into_socket_address(self) -> SocketAddress {
        self.0
    }
}

impl TryFrom<&RawFd> for PeerAddress {
    type Error = NetworkError;

    fn try_from(value: &RawFd) -> Result<Self, Self::Error> {
        let mut addr: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        let mut addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

        syscall!(getpeername(
            *value,
            &mut addr as *mut _ as *mut libc::sockaddr,
            &mut addr_len as *mut _
        )).map_err(|e| {
            NetworkError::SocketGetNameError(e.into())
        })?;

        let socket_address = SocketAddress::try_from(addr)?;

        Ok(PeerAddress(socket_address))
    }
}

impl TryInto<libc::sockaddr_storage> for PeerAddress {
    type Error = NetworkError;
    fn try_into(self) -> Result<libc::sockaddr_storage, NetworkError> {
        self.0.try_into()
    }
}

impl TryFrom<libc::sockaddr_storage> for PeerAddress {
    type Error = NetworkError;
    fn try_from(addr: libc::sockaddr_storage) -> Result<Self, NetworkError> {
        SocketAddress::try_from(addr).map(PeerAddress)
    }
}

impl fmt::Display for PeerAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct LocalAddress(SocketAddress);

impl LocalAddress {

    pub fn ip(&self) -> &IpAddress { self.0.ip() }
    pub fn port(&self) -> u16 { self.0.port() }

    pub fn new(socket_address: SocketAddress) -> Self {
        Self(socket_address)
    }

    pub fn as_socket_address(&self) -> &SocketAddress {
        &self.0
    }

    pub fn into_socket_address(self) -> SocketAddress {
        self.0
    }
}

impl TryFrom<&RawFd> for LocalAddress {
    type Error = NetworkError;

    fn try_from(value: &RawFd) -> Result<Self, Self::Error> {
        let mut addr: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        let mut addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

        syscall!(getsockname(
            *value,
            &mut addr as *mut _ as *mut libc::sockaddr,
            &mut addr_len as *mut _
        )).map_err(|e| {
            NetworkError::SocketGetNameError(e.into())
        })?;

        let socket_address = SocketAddress::try_from(addr)?;

        Ok(LocalAddress(socket_address))
    }
}

impl TryInto<libc::sockaddr_storage> for LocalAddress {
    type Error = NetworkError;
    fn try_into(self) -> Result<libc::sockaddr_storage, NetworkError> {
        self.0.try_into()
    }
}

impl TryFrom<libc::sockaddr_storage> for LocalAddress {
    type Error = NetworkError;
    fn try_from(addr: libc::sockaddr_storage) -> Result<Self, NetworkError> {
        SocketAddress::try_from(addr).map(LocalAddress)
    }
}

impl fmt::Display for LocalAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}