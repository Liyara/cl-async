use std::{net::Ipv4Addr, str::FromStr};

use super::NetworkError;


#[derive(Debug, Clone)]
pub struct Address {
    pub ip: String,
    pub port: u16,
}

impl Address {

    pub fn new(ip: &str, port: u16) -> Self {
        Self {
            ip: ip.to_string(),
            port,
        }
    }
}

impl TryInto<libc::sockaddr_in> for Address {
    type Error = NetworkError;
    fn try_into(self) -> Result<libc::sockaddr_in, NetworkError> {

        let ipv4_addr = Ipv4Addr::from_str(&self.ip).map_err(|e| {
            NetworkError::AddressParseError(e)
        })?;

        Ok(libc::sockaddr_in {
            sin_family: libc::AF_INET as u16,
            sin_port: self.port.to_be(),
            sin_addr: libc::in_addr {
                s_addr: u32::from(ipv4_addr).to_be(),
            },
            sin_zero: [0; 8],
        })
    }
}

impl From<libc::sockaddr_in> for Address {
    fn from(addr: libc::sockaddr_in) -> Self {
        let ip = Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr)).to_string();
        let port = u16::from_be(addr.sin_port);

        Self {
            ip,
            port,
        }
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.ip, self.port)
    }
}