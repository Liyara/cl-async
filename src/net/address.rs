use std::{fmt, os::fd::RawFd};

use thiserror::Error;

use crate::OsError;

#[derive(Debug, Error)]
pub enum IpV4ParseError {
    #[error("Invalid V4 Format: {0}")]
    InvalidV4Format(String),

    #[error("Invalid V4 Value: {0}")]
    InvalidV4Value(String),
}

#[derive(Debug, Error)]
pub enum IpV6CompressError {
    #[error("Invalid Uncompressed V6 Format: {0}")]
    InvalidUncompressedV6Format(String),

    #[error("Invalid Uncompressed V6 Value: {0}")]
    InvalidUncompressedV6Value(String),
}

#[derive(Debug, Error)]
pub enum IpV6UncompressError {
    #[error("Invalid Compressed V6 Format: {0}")]
    InvalidCompressedV6Format(String),

    #[error("Invalid Compressed V6 Value: {0}")]
    InvalidCompressedV6Value(String),
}

#[derive(Debug, Error)]
pub enum V4MappedV6Error {
    #[error("Invalid V4 Mapped V6 Format: {0}")]
    InvalidMappedV6Format(String),

    #[error("Failed to parse V4 Mapped V6 Value: {0}")]
    V4ParseError(#[from] IpV4ParseError),
}

#[derive(Debug, Error)]
pub enum HostnameResolutionError {
    #[error("Invalid CString: {0}")]
    InvalidCString(String),

    #[error("Failed to resolve hostname; {hostname}, message: {message}, code: {code}")]
    ResolutionFailure {
        hostname: String,
        message: String,
        code: i32,
    },
}

#[derive(Debug, Error)]
pub enum IpParseError {
    #[error("Failed to resolve IP address: {0}")]
    NoAddressFound(String),

    #[error("IP V4 Parse Error: {0}")]
    IpV4ParseError(#[from] IpV4ParseError),

    #[error("IP V6 Uncompress Error: {0}")]
    IpV6UncompressError(#[from] IpV6UncompressError),

    #[error("V4 Mapped V6 Error: {0}")]
    V4MappedV6Error(#[from] V4MappedV6Error),

    #[error("Hostname Resolution Error: {0}")]
    HostnameResolutionError(#[from] HostnameResolutionError),
}

#[derive(Debug, Error)]
pub enum IpError {
    #[error("IP Parse Error: {0}")]
    IpParseError(#[from] IpParseError),

    #[error("IP V6 Compress Error: {0}")]
    IpV6CompressError(#[from] IpV6CompressError),
}

pub enum SocketType {
    Stream,
    Datagram,
}

pub struct ResolveHostnamePreferences {
    pub version: Option<IpVersion>,
    pub socket_type: Option<SocketType>,
}

impl Default for ResolveHostnamePreferences {
    fn default() -> Self {
        Self {
            version: None,
            socket_type: None,
        }
    }
}

impl Into<libc::addrinfo> for ResolveHostnamePreferences {
    fn into(self) -> libc::addrinfo {

        let mut hints: libc::addrinfo = unsafe { std::mem::zeroed() };

        hints.ai_family = match self.version {
            Some(IpVersion::V4) => libc::AF_INET,
            Some(IpVersion::V6) => libc::AF_INET6,
            None => libc::AF_UNSPEC,
        };

        hints.ai_socktype = match self.socket_type {
            Some(SocketType::Stream) => libc::SOCK_STREAM,
            Some(SocketType::Datagram) => libc::SOCK_DGRAM,
            None => 0,
        };

        hints
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
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

    // Create an IP address from an IPv4 string
    pub fn v4(addr: &str) -> Result<Self, IpV4ParseError> {
        Self::v4_from_str(addr) 
    }

    // Create an IP address from an IPv6 string; both compressed and uncompressed formats are accepted
    pub fn v6(addr: &str) -> Result<Self, IpParseError> {

        if addr.starts_with("[") && addr.ends_with("]") {
            return Self::v6(&addr[1..addr.len() - 1]);
        }

        if Self::is_mapped_v6(addr) {
            return Ok(Self::parse_mapped_v6(addr)?);
        }

        let uncompressed = if Self::is_compressed_v6(addr) {
            Self::uncompress_v6(addr)?
        } else { addr.to_string() };

        let bytes = Self::uncompressed_v6_as_bytes(&uncompressed)?;

        Ok(IpAddress::V6(bytes))
    }

    pub fn resolve_hostname(
        hostname: &str,
        pref: ResolveHostnamePreferences
    ) -> Result<Vec<Self>, HostnameResolutionError> {

        let c_hostname = std::ffi::CString::new(hostname)
            .map_err(|_| HostnameResolutionError::InvalidCString(
                hostname.to_string()
            )
        )?;

        let hints: libc::addrinfo = pref.into();
        let mut result_ptr: *mut libc::addrinfo = std::ptr::null_mut();

        let ret = unsafe {
            libc::getaddrinfo(
                c_hostname.as_ptr(),
                std::ptr::null(),
                &hints,
                &mut result_ptr
            )
        };

        if ret != 0 {
            let msg = unsafe {
                std::ffi::CStr::from_ptr(libc::gai_strerror(ret))
            }.to_string_lossy().into_owned();

            if !result_ptr.is_null() {
                unsafe { libc::freeaddrinfo(result_ptr) };
            }

            return Err(HostnameResolutionError::ResolutionFailure {
                hostname: hostname.to_string(),
                message: msg,
                code: ret,
            });
        }

        let mut resolved_ips = Vec::new();
        let mut current_addrinfo = result_ptr;

        while !current_addrinfo.is_null() {
            let addrinfo = unsafe { &*current_addrinfo };

            match addrinfo.ai_family {
                libc::AF_INET => {

                    let sockaddr_in = unsafe {
                        &*(addrinfo.ai_addr as *const libc::sockaddr_in)
                    };

                    let ip_bytes = u32::from_be(sockaddr_in.sin_addr.s_addr).to_be_bytes();
                    resolved_ips.push(IpAddress::V4(ip_bytes));
                },
                libc::AF_INET6 => {

                    let sockaddr_in6 = unsafe {
                        &*(addrinfo.ai_addr as *const libc::sockaddr_in6)
                    };

                    resolved_ips.push(IpAddress::V6(sockaddr_in6.sin6_addr.s6_addr));
                },
                _ => ()
            }

            current_addrinfo = addrinfo.ai_next;
        }

        // Free the address info structure
        if !result_ptr.is_null() {
            unsafe { libc::freeaddrinfo(result_ptr) };
        }

        Ok(resolved_ips)
    }

    pub fn is_v4_mapped_v6(&self) -> bool {
        match self {
            IpAddress::V4(_) => false,
            IpAddress::V6(addr) => {
                addr[0..10].iter().all(|&b| b == 0)
                    && addr[10] == 0xff
                    && addr[11] == 0xff
            }
        }
    }

    pub fn any_v4() -> Self { IpAddress::V4([0u8; 4]) }
    pub fn any_v6() -> Self { IpAddress::V6([0u8; 16]) }
    pub fn localhost_v4() -> Self { IpAddress::V4([127, 0, 0, 1]) }

    pub fn localhost_v6() -> Self {
        let mut bytes = [0u8; 16];
        bytes[15] = 1;
        IpAddress::V6(bytes)
    }

    pub fn broadcast_v4() -> Self { IpAddress::V4([255u8; 4]) }
    pub fn multicast_v4() -> Self { IpAddress::V4([224, 0, 0, 1]) }

    pub fn interface_local_v6() -> Self {
        let mut bytes = [0u8; 16];
        bytes[0] = 0xff;
        bytes[1] = 0x01;
        bytes[15] = 0x01;
        IpAddress::V6(bytes)
    }

    pub fn link_local_v6() -> Self {
        let mut bytes = [0u8; 16];
        bytes[0] = 0xff;
        bytes[1] = 0x02;
        bytes[15] = 0x01;
        IpAddress::V6(bytes)
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

    fn v4_bytes_from_str(addr: &str) -> Result<[u8; 4], IpV4ParseError> {
        let parts: Vec<&str> = str::split(addr, '.').collect();

        if parts.len() != 4 {
            return Err(IpV4ParseError::InvalidV4Format(addr.to_string()));
        }

        let mut bytes = [0u8; 4];

        for i in 0..4 {
            let part = parts[i].parse::<u8>().map_err(|_| {
                IpV4ParseError::InvalidV4Value(addr.to_string())
            })?;

            bytes[i] = part;
        }

        Ok(bytes)
    }

    fn v4_from_str(addr: &str) -> Result<Self, IpV4ParseError> {
        
        let bytes = Self::v4_bytes_from_str(addr)?;
        Ok(IpAddress::V4(bytes))
    }

    fn compress_v6(addr: &str) -> Result<String, IpV6CompressError> {

        let mut parts: Vec<String> = str::split(
            addr, 
            ':'
        ).map(|s| s.trim().to_string())
        .collect();

        if parts.len() != 8 {
            return Err(IpV6CompressError::InvalidUncompressedV6Format(addr.to_string()));
        }

        for part in parts.iter_mut() {

            if part.len() == 0 || part.len() > 4 {
                return Err(IpV6CompressError::InvalidUncompressedV6Value(addr.to_string()));
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
                return Err(IpV6CompressError::InvalidUncompressedV6Value(addr.to_string()));
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
        } else if double_colon_index == 0 {
            parts.insert(0, String::default());
        } else if double_colon_index as usize == parts.len() - 1 {
            parts.push(String::default());
        }

        let recombined = parts.join(":");
        Ok(recombined)
        
    }

    /*
        Does not indicate if the adress is a VALID compressed IPv6 address
        Just checks if its formatted in such a way that the intent of the caller
        is for the string to be interpreted as a compressed IPv6 address
    */
    fn is_compressed_v6(addr: &str) -> bool {
        addr.contains("::") && addr.matches(':').count() <= 7
    }

    /*
        Does not indicate if the adress is a VALID mapped IPv6 address
        Just checks if its formatted in such a way that the intent of the caller
        is for the string to be interpreted as a mapped IPv6 address
    */
    fn is_mapped_v6(addr: &str) -> bool {
        addr.starts_with("::ffff:") && addr.contains('.')
    }

    /*
        Does not indicate if the adress is a VALID uncompressed IPv6 address
        Just checks if its formatted in such a way that the intent of the caller
        is for the string to be interpreted as an uncompressed IPv6 address
    */
    fn is_uncompressed_v6(addr: &str) -> bool {
        addr.matches(':').count() == 7 && !addr.contains("::")
    }

    fn is_v6(addr: &str) -> bool {
        Self::is_compressed_v6(addr) || Self::is_uncompressed_v6(addr) || Self::is_mapped_v6(addr)
    }

    fn is_v4(addr: &str) -> bool {
        addr.matches('.').count() == 3 && !addr.contains(':')
    }

    fn parse_mapped_v6(addr: &str) -> Result<Self, V4MappedV6Error> {

        let v4_str = addr.split_once("::ffff:").ok_or(
            V4MappedV6Error::InvalidMappedV6Format(addr.to_string())
        )?.1;

        let v4_bytes = Self::v4_bytes_from_str(v4_str)?;

        let mut bytes = [0u8; 16];
        bytes[10] = 0xff;
        bytes[11] = 0xff;
        bytes[12..].copy_from_slice(&v4_bytes);

        Ok(IpAddress::V6(bytes))
    }

    fn uncompress_v6(addr: &str) -> Result<String, IpV6UncompressError> {
        let mut parts: Vec<String> = str::split(
            addr, 
            ':'
        ).map(|s| s.trim().to_string())
        .collect();

        if parts.len() > 9 || parts.len() < 3 {
            return Err(IpV6UncompressError::InvalidCompressedV6Format(addr.to_string()));
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
                return Err(IpV6UncompressError::InvalidCompressedV6Value(addr.to_string()));
            }

            if part.is_empty() {
                if double_colon_index == -1 {
                    double_colon_index = i as isize;
                    continue;
                } else {
                    return Err(IpV6UncompressError::InvalidCompressedV6Value(addr.to_string()));
                }
            }

            while part.len() < 4 {
                part.insert(0, '0');
            }
        }

        if double_colon_index == -1 {
            if parts.len() != 8 {
                return Err(IpV6UncompressError::InvalidCompressedV6Format(addr.to_string()));
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

    fn uncompressed_v6_as_bytes(addr: &str) -> Result<[u8; 16], IpV6UncompressError> {
        let mut bytes = [0u8; 16];
        let parts: Vec<&str> = str::split(addr, ':').collect();

        if parts.len() != 8 {
            return Err(IpV6UncompressError::InvalidCompressedV6Format(addr.to_string()));
        }

        for i in 0..8 {
            let part = u16::from_str_radix(parts[i], 16).map_err(|_| {
                IpV6UncompressError::InvalidCompressedV6Value(addr.to_string())
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

impl From<[u8; 4]> for IpAddress {
    fn from(addr: [u8; 4]) -> Self {
        IpAddress::V4(addr)
    }
}

impl From<[u8; 16]> for IpAddress {
    fn from(addr: [u8; 16]) -> Self {
        IpAddress::V6(addr)
    }
}

impl TryFrom<&str> for IpAddress {
    type Error = IpParseError;
    fn try_from(ip: &str) -> Result<Self, Self::Error> {
        let ip_formatted = ip.trim();
        
        if Self::is_v4(ip_formatted) {
            Ok(Self::v4(ip_formatted)?)
        } else if Self::is_v6(ip_formatted) {
            Self::v6(ip_formatted)
        } else {
            let ips = Self::resolve_hostname(
                ip_formatted,
                ResolveHostnamePreferences::default()
            )?;

            if ips.is_empty() {
                Err(IpParseError::NoAddressFound(ip_formatted.to_string()))
            } else {
                Ok(ips[0])
            }
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
                if self.is_v4_mapped_v6() {
                    let v4_bytes = &addr[12..];
                    return write!(f, "::ffff:{}.{}.{}.{}", v4_bytes[0], v4_bytes[1], v4_bytes[2], v4_bytes[3]);
                }

                let uncompressed = Self::bytes_as_uncompressed_v6(addr);
                let compressed = Self::compress_v6(&uncompressed).unwrap_or_else(|_| uncompressed.clone());
                write!(f, "{}", compressed)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Port(u16);

impl Port {
    pub fn ftp_data() -> Self { Port(20) }
    pub fn ftp_control() -> Self { Port(21) }
    pub fn ssh() -> Self { Port(22) }
    pub fn telnet() -> Self { Port(23) }
    pub fn smtp() -> Self { Port(25) }
    pub fn dns() -> Self { Port(53) }
    pub fn http() -> Self { Port(80) }
    pub fn pop3() -> Self { Port(110) }
    pub fn imap() -> Self { Port(143) }
    pub fn snmp() -> Self { Port(161) }
    pub fn snmp_trap() -> Self { Port(162) }
    pub fn ldap() -> Self { Port(389) }
    pub fn http_dev() -> Self { Port(3000) }
    pub fn https() -> Self { Port(443) }
    pub fn smb() -> Self { Port(445) }
    pub fn imaps() -> Self { Port(993) }
    pub fn pop3s() -> Self { Port(995) }
    pub fn mssql() -> Self { Port(1433) }
    pub fn mssql_alt() -> Self { Port(1434) }
    pub fn oracle() -> Self { Port(1521) }
    pub fn mysql() -> Self { Port(3306) }
    pub fn rdp() -> Self { Port(3389) }
    pub fn postgres() -> Self { Port(5432) }
    pub fn redis() -> Self { Port(6379) }
    pub fn http_alt() -> Self { Port(8080) }
    pub fn mongodb() -> Self { Port(27017) }
    pub fn db2() -> Self { Port(50000) }

    pub fn to_be(&self) -> u16 { self.0.to_be() }
    pub fn to_le(&self) -> u16 { self.0.to_le() }

    pub fn from_be(port: u16) -> Self { Port(u16::from_be(port)) }
    pub fn from_le(port: u16) -> Self { Port(u16::from_le(port)) }
}

impl From<u16> for Port {
    fn from(port: u16) -> Self {
        Port(port)
    }
}

impl Into<u16> for Port {
    fn into(self) -> u16 {
        self.0
    }
}

impl fmt::Display for Port {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SocketAddress {
    ip: IpAddress,
    port: Port,
}

impl SocketAddress {

    pub fn ip(&self) -> &IpAddress { &self.ip }
    pub fn port(&self) -> Port { self.port }

    pub fn new(ip: IpAddress, port: Port) -> Self {
        Self { ip, port }
    }
}

impl Into<libc::sockaddr_storage> for SocketAddress {
    fn into(self) -> libc::sockaddr_storage {
        match self.ip {
            IpAddress::V4(be_bytes) => {

                let addr_in = libc::sockaddr_in {
                    sin_family: libc::AF_INET as u16,
                    sin_port: self.port.to_be(),
                    sin_addr: libc::in_addr {
                        s_addr: u32::from_be_bytes(be_bytes).to_be(),
                    },
                    sin_zero: [0; 8],
                };

                let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };

                unsafe {
                    let storage_ptr = &mut storage as *mut _ as *mut libc::sockaddr_in;
                    std::ptr::write(storage_ptr, addr_in);
                }

                storage
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

                storage
            }
        }
    }
}

#[derive(Debug, Error)]
#[error("Unsupported address family: {0}")]
pub struct UnsupportedAddressFamilyError(libc::sa_family_t);

impl TryFrom<libc::sockaddr_storage> for SocketAddress {
    type Error = UnsupportedAddressFamilyError;
    fn try_from(addr: libc::sockaddr_storage) -> Result<Self, UnsupportedAddressFamilyError> {
        let version = addr.ss_family;

        if version == libc::AF_INET as libc::sa_family_t {
            let addr_in = unsafe {
                *(&addr as *const libc::sockaddr_storage as *const libc::sockaddr_in)
            };

            Ok(Self {
                ip: IpAddress::V4(u32::from_be(addr_in.sin_addr.s_addr).to_be_bytes()),
                port: Port::from_be(addr_in.sin_port),
            })
        } else if version == libc::AF_INET6 as libc::sa_family_t {
            let addr_in6 = unsafe {
                *(&addr as *const libc::sockaddr_storage as *const libc::sockaddr_in6)
            };

            Ok(Self {
                ip: IpAddress::V6(addr_in6.sin6_addr.s6_addr),
                port: Port::from_be(addr_in6.sin6_port),
            })
        } else {
            Err(UnsupportedAddressFamilyError(version))
        }
    }
}

impl std::fmt::Display for SocketAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}

#[derive(Debug, Error)]
pub enum AddressRetrievalError {
    #[error("Incurred OS error while attempting to retrieve peer address: {0}")]
    OsError(#[from] OsError),

    #[error("Retrieved address does not belong to a known address family: {0}")]
    InvalidSocketAddress(#[from] UnsupportedAddressFamilyError),
}

impl TryFrom<&RawFd> for SocketAddress {
    type Error = AddressRetrievalError;

    fn try_from(value: &RawFd) -> Result<Self, Self::Error> {
        let mut addr: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        let mut addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

        syscall!(getsockname(
            *value,
            &mut addr as *mut _ as *mut libc::sockaddr,
            &mut addr_len as *mut _
        )).map_err(OsError::from)?;

        Ok(SocketAddress::try_from(addr)?)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PeerAddress(SocketAddress);

impl PeerAddress {

    pub fn ip(&self) -> &IpAddress { self.0.ip() }
    pub fn port(&self) -> Port { self.0.port() }

    pub fn new(ip: IpAddress, port: Port) -> Self {
        Self(SocketAddress::new(ip, port))
    }

    pub fn as_socket_address(&self) -> &SocketAddress {
        &self.0
    }

    pub fn into_socket_address(self) -> SocketAddress {
        self.0
    }
}

impl TryFrom<&RawFd> for PeerAddress {
    type Error = AddressRetrievalError;

    fn try_from(value: &RawFd) -> Result<Self, Self::Error> {
        Ok(PeerAddress(SocketAddress::try_from(value)?))
    }
}

impl From<SocketAddress> for PeerAddress {
    fn from(socket_address: SocketAddress) -> Self {
        PeerAddress(socket_address)
    }
}

impl Into<libc::sockaddr_storage> for PeerAddress {
    fn into(self) -> libc::sockaddr_storage {
        self.0.into()
    }
}

impl TryFrom<libc::sockaddr_storage> for PeerAddress {
    type Error = UnsupportedAddressFamilyError;
    fn try_from(addr: libc::sockaddr_storage) -> Result<Self, UnsupportedAddressFamilyError> {
        SocketAddress::try_from(addr).map(PeerAddress)
    }
}

impl fmt::Display for PeerAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LocalAddress(SocketAddress);

impl LocalAddress {

    pub fn ip(&self) -> &IpAddress { self.0.ip() }
    pub fn port(&self) -> Port { self.0.port() }

    pub fn new(ip: IpAddress, port: Port) -> Self {
        Self(SocketAddress::new(ip, port))
    }

    pub fn as_socket_address(&self) -> &SocketAddress {
        &self.0
    }

    pub fn into_socket_address(self) -> SocketAddress {
        self.0
    }

    pub fn any_v4(port: Port) -> Self {
        Self(SocketAddress::new(
            IpAddress::any_v4(),
            port,
        ))
    }

    pub fn any_v6(port: Port) -> Self {
        Self(SocketAddress::new(
            IpAddress::any_v6(),
            port,
        ))
    }

    pub fn localhost_v4(port: Port) -> Self {
        Self(SocketAddress::new(
            IpAddress::localhost_v4(),
            port,
        ))
    }

    pub fn localhost_v6(port: Port) -> Self {
        Self(SocketAddress::new(
            IpAddress::localhost_v6(),
            port,
        ))
    }

    pub fn broadcast_v4(port: Port) -> Self {
        Self(SocketAddress::new(
            IpAddress::broadcast_v4(),
            port,
        ))
    }

    pub fn multicast_v4(port: Port) -> Self {
        Self(SocketAddress::new(
            IpAddress::multicast_v4(),
            port,
        ))
    }

    pub fn interface_local_v6(port: Port) -> Self {
        Self(SocketAddress::new(
            IpAddress::interface_local_v6(),
            port,
        ))
    }

    pub fn link_local_v6(port: Port) -> Self {
        Self(SocketAddress::new(
            IpAddress::link_local_v6(),
            port,
        ))
    }
    
}

impl TryFrom<&RawFd> for LocalAddress {
    type Error = AddressRetrievalError;

    fn try_from(value: &RawFd) -> Result<Self, Self::Error> {
        Ok(LocalAddress(SocketAddress::try_from(value)?))
    }
}

impl From<SocketAddress> for LocalAddress {
    fn from(socket_address: SocketAddress) -> Self {
        LocalAddress(socket_address)
    }
}

impl Into<libc::sockaddr_storage> for LocalAddress {
    fn into(self) -> libc::sockaddr_storage {
        self.0.into()
    }
}

impl TryFrom<libc::sockaddr_storage> for LocalAddress {
    type Error = UnsupportedAddressFamilyError;
    fn try_from(addr: libc::sockaddr_storage) -> Result<Self, Self::Error> {
        SocketAddress::try_from(addr).map(LocalAddress)
    }
}

impl fmt::Display for LocalAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Import items from the parent module


    // --- Port Tests ---

    #[test]
    fn test_port_creation_and_value() {
        let p1 = Port::from(8080u16);
        let p2 = Port(8080); // Direct construction if pub
        assert_eq!(p1, p2);
        assert_eq!(p1.0, 8080); // Access inner value (if needed, often avoid)
        let val: u16 = p1.into();
        assert_eq!(val, 8080);
    }

    #[test]
    fn test_port_display() {
        let port = Port::from(1234);
        assert_eq!(format!("{}", port), "1234");
    }

    #[test]
    fn test_port_helpers() {
        assert_eq!(Port::http(), Port(80));
        assert_eq!(Port::https(), Port(443));
        assert_eq!(Port::ssh(), Port(22));
    }

    #[test]
    fn test_port_byte_order() {
        let port_val = 8080u16; // Example port
        let port = Port::from(port_val);
        assert_eq!(port.to_be(), port_val.to_be());
        assert_eq!(Port::from_be(port_val.to_be()), port);
    }

    // --- IpAddress Tests ---

    #[test]
    fn test_ipv4_parsing_valid() {
        let ip_str = "192.168.1.1";
        let expected_bytes = [192, 168, 1, 1];
        let ip = IpAddress::v4(ip_str).unwrap();
        assert_eq!(ip, IpAddress::V4(expected_bytes));
        assert_eq!(ip.version(), IpVersion::V4);
    }

    #[test]
    fn test_ipv4_parsing_invalid_format() {
        assert!(IpAddress::v4("192.168.1").is_err());
        assert!(IpAddress::v4("192.168.1.1.1").is_err());
        assert!(IpAddress::v4("192.168..1").is_err());
        assert!(IpAddress::v4("hello").is_err());
    }

    #[test]
    fn test_ipv4_parsing_invalid_value() {
        assert!(IpAddress::v4("192.168.1.256").is_err());
        assert!(IpAddress::v4("192.-1.1.1").is_err()); // Assuming parse::<u8> fails
    }

    #[test]
    fn test_ipv6_parsing_valid_uncompressed() {
        let ip_str = "2001:0db8:85a3:0000:0000:8a2e:0370:7334";
        let ip = IpAddress::v6(ip_str).unwrap();
        assert_eq!(ip.version(), IpVersion::V6);
        // We'll test the bytes via display/compression tests later
    }

    #[test]
    fn test_ipv6_parsing_valid_compressed() {
        assert!(IpAddress::v6("2001:db8::1").is_ok());
        assert!(IpAddress::v6("::1").is_ok()); // Loopback
        assert!(IpAddress::v6("::").is_ok()); // Unspecified
        assert!(IpAddress::v6("1::").is_ok()); // Starts with non-zero, ends with ::
        assert!(IpAddress::v6("ff02::1").is_ok()); // Link-local multicast
    }

     #[test]
    fn test_ipv6_parsing_invalid_format() {
        assert!(IpAddress::v6("::1::2").is_err()); // Multiple ::
        assert!(IpAddress::v6("1:2:3:4:5:6:7:8:9").is_err()); // Too many parts
        assert!(IpAddress::v6("1:2:3").is_err()); // Too few parts (without ::)
        assert!(IpAddress::v6("1:2:3:4:5:6:7:ghij").is_err()); // Invalid hex
        assert!(IpAddress::v6("12345::1").is_err()); // Part too long
    }

    #[test]
    fn test_ipaddress_display_v4() {
        let ip = IpAddress::V4([10, 0, 0, 1]);
        assert_eq!(format!("{}", ip), "10.0.0.1");
    }

    #[test]
    fn test_ipaddress_display_v6_compression() {
        // Test cases rely on your compress_v6 implementation being correct
        let test_cases = [
            // Input bytes             Expected Compressed String
            ([0u8; 16], "::"), // Unspecified
            ([0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1], "::1"), // Loopback
            ([0x20,0x01,0x0d,0xb8,0,0,0,0,0,0,0,0,0,0,0,1], "2001:db8::1"),
            ([0x20,0x01,0x0d,0xb8,0x85,0xa3,0,0,0,0,0x8a,0x2e,0x03,0x70,0x73,0x34], "2001:db8:85a3::8a2e:370:7334"),
            ([0xff,0x02,0,0,0,0,0,0,0,0,0,0,0,0,0,1], "ff02::1"), // Link-local multi
            ([0,0,0,0,0,0,0,0,0,0,0xff,0xff, 192, 168, 0, 1], "::ffff:192.168.0.1"), // V4 Mapped (Note: Your display doesn't explicitly format this way, it shows as pure V6)
            // Add more edge cases for compression rules if possible
            ([0x20,0x01,0,0,0,0,0,0,0,0,0,0,0,0,0,0x01], "2001::1"), // Zeroes in middle
            ([0,0,0,0,0,0,0,0,0,0,0,0,0x01,0x02,0x03,0x04], "::102:304"), // Zeroes at start
            ([0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], "1::"), // Zeroes at end
            ([0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0], "1::1:0")
        ];

        for (bytes, expected_str) in test_cases {
            let ip = IpAddress::V6(bytes);
            let display_str = format!("{}", ip);

            assert_eq!(display_str, expected_str, "Display for {:?} failed", bytes);

            // Also test TryFrom<String> for the expected string
            let parsed_back = IpAddress::try_from(expected_str);
             // Handle the v4-mapped case where parsing might differ
            if expected_str != "::ffff:192.168.0.1" { // Skip this specific string as it's not standard v6 format
                 assert!(parsed_back.is_ok(), "Failed to parse back '{}'", expected_str);
                 // Comparing bytes is the most reliable way to check correctness after parsing
                 match parsed_back.unwrap() {
                    IpAddress::V4(_) => panic!("Parsed V6 string as V4"),
                    IpAddress::V6(parsed_bytes) => assert_eq!(bytes, parsed_bytes, "Byte mismatch after parsing '{}'", expected_str),
                 }
            }
        }
    }

    #[test]
    fn test_ipaddress_helpers() {
        assert_eq!(IpAddress::any_v4(), IpAddress::V4([0, 0, 0, 0]));
        assert_eq!(IpAddress::localhost_v4(), IpAddress::V4([127, 0, 0, 1]));
        assert_eq!(IpAddress::broadcast_v4(), IpAddress::V4([255, 255, 255, 255]));
        assert_eq!(IpAddress::multicast_v4(), IpAddress::V4([224, 0, 0, 1]));

        assert_eq!(IpAddress::any_v6(), IpAddress::V6([0; 16]));
        let mut lh_bytes = [0u8; 16]; lh_bytes[15] = 1;
        assert_eq!(IpAddress::localhost_v6(), IpAddress::V6(lh_bytes));
        // Add checks for interface_local_v6, link_local_v6 if desired
    }

     #[test]
    fn test_ipaddress_try_from_string() {
        // V4
        let ip_v4_str = "192.168.10.1";
        let ip_v4 = IpAddress::try_from(ip_v4_str).unwrap();
        assert_eq!(ip_v4, IpAddress::V4([192, 168, 10, 1]));

        // V6
        let ip_v6_str = "::1";
        let ip_v6 = IpAddress::try_from(ip_v6_str).unwrap();
        let mut lh_bytes = [0u8; 16]; lh_bytes[15] = 1;
        assert_eq!(ip_v6, IpAddress::V6(lh_bytes));

        // Invalid
        let invalid_str = "not an ip";
        assert!(IpAddress::try_from(invalid_str).is_err());
        let invalid_str_dots_colon = "192.168.0.1::1"; // Contains both
         // Your current logic might parse this as V4. Test the actual behavior.
        // assert!(IpAddress::try_from(invalid_str_dots_colon).is_err()); // Or assert specific V4/V6 result if that's intended
         let result = IpAddress::try_from(invalid_str_dots_colon);
         // Example: Check if it parsed as V4 because '.' was present
         assert!(result.is_err());


    }

    #[test]
    fn test_ipaddress_to_be_bytes() {
        // V4 mapped
        let ip_v4 = IpAddress::V4([192, 168, 0, 1]);
        let expected_v4_mapped: [u8; 16] = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 192, 168, 0, 1,
        ];
        assert_eq!(ip_v4.to_be_bytes(), expected_v4_mapped);

        // V6
        let ip_v6 = IpAddress::V6([0x20,0x01,0x0d,0xb8,0,0,0,0,0,0,0,0,0,0,0,1]);
        assert_eq!(ip_v6.to_be_bytes(), [0x20,0x01,0x0d,0xb8,0,0,0,0,0,0,0,0,0,0,0,1]);
    }


    // --- SocketAddress Tests ---

    #[test]
    fn test_socketaddress_creation_accessors() {
        let ip = IpAddress::localhost_v4();
        let port = Port::http();
        let sa = SocketAddress::new(ip, port);
        assert_eq!(sa.ip(), &ip);
        assert_eq!(sa.port(), port);
    }

    #[test]
    fn test_socketaddress_display() {
        let sa_v4 = SocketAddress::new(IpAddress::V4([127, 0, 0, 1]), Port(8080));
        assert_eq!(format!("{}", sa_v4), "127.0.0.1:8080");

        let sa_v6 = SocketAddress::new(IpAddress::V6([0; 16]), Port(443));
        assert_eq!(format!("{}", sa_v6), ":::443"); // Relies on IpAddress Display

        let sa_v6_loopback = SocketAddress::new(IpAddress::localhost_v6(), Port(80));
         assert_eq!(format!("{}", sa_v6_loopback), "::1:80");
    }

    // --- libc Interop Tests ---

    #[test]
    fn test_socketaddress_libc_roundtrip_v4() {
        let ip = IpAddress::V4([192, 168, 50, 60]);
        let port = Port(12345);
        let original_sa = SocketAddress::new(ip, port);

        // Convert to libc::sockaddr_storage
        let storage_res: Result<libc::sockaddr_storage, _> = original_sa.clone().try_into();
        assert!(storage_res.is_ok());
        let storage = storage_res.unwrap();

        // Check family and port (careful with alignment and unsafe)
        assert_eq!(storage.ss_family as i32, libc::AF_INET);
        let sockaddr_in_ptr = &storage as *const _ as *const libc::sockaddr_in;
        // SAFETY: We checked ss_family is AF_INET, so casting to sockaddr_in is valid.
        // Reading sin_port and sin_addr is safe as they are part of the struct.
        let port_be = unsafe { (*sockaddr_in_ptr).sin_port };
        let addr_be = unsafe { (*sockaddr_in_ptr).sin_addr.s_addr };
        assert_eq!(port_be, port.to_be());
        assert_eq!(addr_be, u32::from_be_bytes([192, 168, 50, 60]).to_be());


        // Convert back to SocketAddress
        let roundtrip_sa_res = SocketAddress::try_from(storage);
        assert!(roundtrip_sa_res.is_ok());
        let roundtrip_sa = roundtrip_sa_res.unwrap();

        // Compare
        assert_eq!(original_sa, roundtrip_sa);
    }

     #[test]
    fn test_socketaddress_libc_roundtrip_v6() {
        let ip_bytes = [0x20,0x01,0x0d,0xb8,0,0,0,1,0,2,0,3,0,4,0,5];
        let ip = IpAddress::V6(ip_bytes);
        let port = Port(54321);
        let original_sa = SocketAddress::new(ip, port);

        // Convert to libc::sockaddr_storage
        let storage_res: Result<libc::sockaddr_storage, _> = original_sa.clone().try_into();
        assert!(storage_res.is_ok());
        let storage = storage_res.unwrap();

        // Check family and port (careful with alignment and unsafe)
        assert_eq!(storage.ss_family as i32, libc::AF_INET6);
        let sockaddr_in6_ptr = &storage as *const _ as *const libc::sockaddr_in6;
         // SAFETY: We checked ss_family is AF_INET6, so casting to sockaddr_in6 is valid.
        // Reading sin6_port and sin6_addr is safe as they are part of the struct.
        let port_be = unsafe { (*sockaddr_in6_ptr).sin6_port };
        let addr_bytes = unsafe { (*sockaddr_in6_ptr).sin6_addr.s6_addr };
        assert_eq!(port_be, port.to_be());
        assert_eq!(addr_bytes, ip_bytes); // s6_addr is already [u8; 16]

        // Convert back to SocketAddress
        let roundtrip_sa_res = SocketAddress::try_from(storage);
        assert!(roundtrip_sa_res.is_ok());
        let roundtrip_sa = roundtrip_sa_res.unwrap();

        // Compare
        assert_eq!(original_sa, roundtrip_sa);
    }

    // --- LocalAddress / PeerAddress Tests ---
    // These mostly rely on SocketAddress working, so just basic checks

    #[test]
    fn test_local_address_helpers() {
        let la = LocalAddress::any_v4(Port(80));
        assert_eq!(la.ip(), &IpAddress::any_v4());
        assert_eq!(la.port(), Port(80));
        assert_eq!(format!("{}", la), "0.0.0.0:80");
    }

     #[test]
    fn test_peer_address_creation() {
        let pa = PeerAddress::from(
            SocketAddress::new(IpAddress::localhost_v6(), Port::https())
        );
        assert_eq!(pa.ip(), &IpAddress::localhost_v6());
        assert_eq!(pa.port(), Port::https());
        assert_eq!(format!("{}", pa), "::1:443");
    }

    #[test]
    fn test_v4_mapped_v6_parsing_and_display() {
        // This input is a valid IPv4-mapped IPv6 address.
        let input = "::ffff:192.168.0.1";
        let ip = IpAddress::v6(input).unwrap();

        // Verify the internal representation: should be an IPv6 variant with bytes:
        // - Bytes 0-9 are 0,
        // - Bytes 10-11 are 0xff,
        // - Bytes 12-15 equal [192, 168, 0, 1].
        match ip {
            IpAddress::V6(bytes) => {
                assert_eq!(
                    &bytes[0..10],
                    &[0u8; 10],
                    "The first 10 bytes should be zeros."
                );
                assert_eq!(
                    &bytes[10..12],
                    &[0xff, 0xff],
                    "Bytes 10-11 should equal 0xff 0xff."
                );
                assert_eq!(
                    &bytes[12..16],
                    &[192, 168, 0, 1],
                    "The last 4 bytes should match the IPv4 address."
                );
            }
            _ => panic!("Expected an IPv6 variant for a mapped address."),
        }

        // Verify that the Display impl produces the canonical mapped string.
        assert_eq!(format!("{}", ip), "::ffff:192.168.0.1");
    }

    #[test]
    fn test_invalid_v4_mapped_v6_value() {
        // Provide an invalid IPv4 component (256 is out-of-range).
        let input = "::ffff:256.0.0.1";
        let result = IpAddress::v6(input);
        assert!(
            result.is_err(),
            "An invalid IPv4 value in a mapped V6 address must result in an error."
        );
    }

    #[test]
    fn test_v4_mapped_v6_try_from_string() {
        let s = "::ffff:10.0.0.1";
        let ip = IpAddress::try_from(s).unwrap();

        // Verify that TryFrom<String> parses a mapped IPv6 address properly.
        match ip {
            IpAddress::V6(bytes) => {
                // Ensure that our helper method recognizes the address as mapped.
                assert!(
                    ip.is_v4_mapped_v6(),
                    "The address should be recognized as an IPv4-mapped IPv6 address."
                );
                assert_eq!(
                    &bytes[12..16],
                    &[10, 0, 0, 1],
                    "The IPv4 component must be parsed correctly."
                );
            }
            _ => panic!("Expected a mapped IPv6 address, not a V4 variant."),
        }
        // Also verify display formatting.
        assert_eq!(format!("{}", ip), "::ffff:10.0.0.1");
    }
}