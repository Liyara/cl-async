use std::{fmt, os::fd::RawFd, pin::Pin, ptr::read_unaligned};

use crate::net::{IpAddress, IpVersion, NetworkError, PeerAddress};

use super::{buffers::IoDoubleBuffer, completion::TryFromCompletion, operation::future::IoOperationFuture, operation_data::IoRecvMsgOutputFlags, IoCompletion, IoDoubleInputBuffer, IoDoubleOutputBuffer, IoError, IoInputBuffer, IoOperationError};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum TlsAlertLevel {
    Warning = 1,
    Fatal = 2,
    Unknown = 255,
}

impl From<u8> for TlsAlertLevel {
    fn from(byte: u8) -> Self {
        match byte {
            1 => TlsAlertLevel::Warning,
            2 => TlsAlertLevel::Fatal,
            _ => TlsAlertLevel::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum TlsAlertDescription {
    CloseNotify = 0,
    UnexpectedMessage = 10,
    BadRecordMac = 20,
    DecryptionFailed = 21,
    RecordOverflow = 22,
    DecompressionFailure = 30,
    HandshakeFailure = 40,
    NoCertificate = 41,
    BadCertificate = 42,
    UnsupportedCertificate = 43,
    CertificateRevoked = 44,
    CertificateExpired = 45,
    CertificateUnknown = 46,
    IllegalParameter = 47,
    UnknownCA = 48,
    AccessDenied = 49,
    DecodeError = 50,
    DecryptError = 51,
    ExportRestriction = 60,
    ProtocolVersion = 70,
    InsufficientSecurity = 71,
    InternalError = 80,
    InappropriateFallback = 86,
    UserCanceled = 90,
    NoRenegotiation = 100,
    MissingExtension = 109,
    UnsupportedExtension = 110,
    CertificateUnobtainable = 111,
    UnrecognizedName = 112,
    BadCertificateStatusResponse = 113,
    BadCertificateHashValue = 114,
    UnknownPSKIdentity = 115,
    CertificateRequired = 116,
    NoApplicationProtocol = 120,
    Unknown = 255,
}

impl From<u8> for TlsAlertDescription {
    fn from(byte: u8) -> Self {
        match byte {
            0 => TlsAlertDescription::CloseNotify,
            10 => TlsAlertDescription::UnexpectedMessage,
            20 => TlsAlertDescription::BadRecordMac,
            21 => TlsAlertDescription::DecryptionFailed,
            22 => TlsAlertDescription::RecordOverflow,
            30 => TlsAlertDescription::DecompressionFailure,
            40 => TlsAlertDescription::HandshakeFailure,
            41 => TlsAlertDescription::NoCertificate,
            42 => TlsAlertDescription::BadCertificate,
            43 => TlsAlertDescription::UnsupportedCertificate,
            44 => TlsAlertDescription::CertificateRevoked,
            45 => TlsAlertDescription::CertificateExpired,
            46 => TlsAlertDescription::CertificateUnknown,
            47 => TlsAlertDescription::IllegalParameter,
            48 => TlsAlertDescription::UnknownCA,
            49 => TlsAlertDescription::AccessDenied,
            50 => TlsAlertDescription::DecodeError,
            51 => TlsAlertDescription::DecryptError,
            60 => TlsAlertDescription::ExportRestriction,
            70 => TlsAlertDescription::ProtocolVersion,
            71 => TlsAlertDescription::InsufficientSecurity,
            80 => TlsAlertDescription::InternalError,
            86 => TlsAlertDescription::InappropriateFallback,
            90 => TlsAlertDescription::UserCanceled,
            100 => TlsAlertDescription::NoRenegotiation,
            109 => TlsAlertDescription::MissingExtension,
            110 => TlsAlertDescription::UnsupportedExtension,
            111 => TlsAlertDescription::CertificateUnobtainable,
            112 => TlsAlertDescription::UnrecognizedName,
            113 => TlsAlertDescription::BadCertificateStatusResponse,
            114 => TlsAlertDescription::BadCertificateHashValue,
            115 => TlsAlertDescription::UnknownPSKIdentity,
            116 => TlsAlertDescription::CertificateRequired,
            120 => TlsAlertDescription::NoApplicationProtocol,
            _ => TlsAlertDescription::Unknown
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TlsAlert {
    pub level: TlsAlertLevel,
    pub description: Option<TlsAlertDescription>,
}

impl fmt::Display for TlsAlert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TlsAlert {{ level: {:?}, description: {:?} }}", self.level, self.description)
    }
}

impl TlsAlert {
    pub fn is_fatal(&self) -> bool { self.level == TlsAlertLevel::Fatal }
    pub fn is_warning(&self) -> bool { self.level == TlsAlertLevel::Warning }
    pub fn is_close_notify(&self) -> bool { 
        self.description == Some(TlsAlertDescription::CloseNotify) 
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Credentials {
    pub process_id: u32,
    pub user_id: u32,
    pub group_id: u32,
}

impl From<libc::ucred> for Credentials {
    fn from(ucred: libc::ucred) -> Self {
        Self {
            process_id: ucred.pid as u32,
            user_id: ucred.uid as u32,
            group_id: ucred.gid as u32,
        }
    }
}

impl Into<libc::ucred> for Credentials {
    fn into(self) -> libc::ucred {
        libc::ucred {
            pid: self.process_id as i32,
            uid: self.user_id as u32,
            gid: self.group_id as u32,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PacketInfo {
    pub interface_index: u32,
    pub destination: IpAddress,
    pub specific_destination: Option<IpAddress>
}

pub enum LibCPktInfo {
    V4(libc::in_pktinfo),
    V6(libc::in6_pktinfo),
}

impl From<libc::in_pktinfo> for PacketInfo {
    fn from(pktinfo: libc::in_pktinfo) -> Self {
        Self {
            interface_index: pktinfo.ipi_ifindex as u32,
            destination: IpAddress::V4(pktinfo.ipi_addr.s_addr.to_be_bytes()),
            specific_destination: Some(IpAddress::V4(pktinfo.ipi_spec_dst.s_addr.to_be_bytes()))
        }
    }
}

impl From<libc::in6_pktinfo> for PacketInfo {
    fn from(pktinfo: libc::in6_pktinfo) -> Self {
        Self {
            interface_index: pktinfo.ipi6_ifindex as u32,
            destination: IpAddress::V6(pktinfo.ipi6_addr.s6_addr),
            specific_destination: None
        }
    }
}

impl TryInto<LibCPktInfo> for PacketInfo {
    
    type Error = NetworkError;
    
    fn try_into(self) -> Result<LibCPktInfo, Self::Error> {
        match self.destination {
            IpAddress::V4(be_bytes) => {
                let pktinfo = libc::in_pktinfo {
                    ipi_ifindex: self.interface_index as i32,
                    ipi_addr: libc::in_addr { 
                        s_addr: u32::from_be_bytes(be_bytes) 
                    },
                    ipi_spec_dst: match self.specific_destination {
                        Some(IpAddress::V4(be_bytes)) => libc::in_addr { 
                            s_addr: u32::from_be_bytes(be_bytes) 
                        },
                        _ => return Err(NetworkError::InvalidAddressByteData),
                    }
                };
                Ok(LibCPktInfo::V4(pktinfo))
            },
            IpAddress::V6(be_bytes) => {
                let pktinfo = libc::in6_pktinfo {
                    ipi6_ifindex: self.interface_index as u32,
                    ipi6_addr: libc::in6_addr {
                        s6_addr: be_bytes,
                    },
                };
                Ok(LibCPktInfo::V6(pktinfo))
            }
        }
    }
}


#[derive(Debug)]
struct RawControlMessageIn<'a> {
    pub message_level: libc::c_int,
    pub message_type: libc::c_int,
    pub data_len: usize,
    pub data_ptr: *const libc::c_uchar,
    _lifetime: std::marker::PhantomData<&'a ()>,
}

#[derive(Debug)]
struct RawControlMessageOut {
    pub message_level: libc::c_int,
    pub message_type: libc::c_int,
    pub payload: Vec<u8>,
}

unsafe fn fill_msg_out_multi<T>(
    msg: &mut RawControlMessageOut,
    message_type: libc::c_int,
    data: &[T],
) where T: Copy + Sized { 
    let data_len = std::mem::size_of::<T>() * data.len();
    msg.message_type = message_type;
    msg.payload.resize(data_len, 0);
    let dest_ptr = msg.payload.as_mut_ptr() as *mut T;

    unsafe { 
        std::ptr::copy_nonoverlapping(data.as_ptr(), dest_ptr, data.len());
    }
}

unsafe fn fill_msg_out_single<T>(
    msg: &mut RawControlMessageOut,
    message_type: libc::c_int,
    data: &T,
) where T: Sized + Copy {
    let data_len = std::mem::size_of::<T>();
    msg.message_type = message_type;
    msg.payload.resize(data_len, 0);
    let data_ptr = msg.payload.as_mut_ptr() as *mut T;
    unsafe {
        std::ptr::copy_nonoverlapping(
            data,
            data_ptr,
            1,
        );
    }
}

unsafe fn from_msg_in_multi<T>(
    msg: &RawControlMessageIn, n: usize
) -> Result<Vec<T>, NetworkError> where T: Copy + Sized {
    let expected_len = std::mem::size_of::<T>() * n;
    if msg.data_len != expected_len {
        return Err(NetworkError::InvalidControlMessageDataSize(
            msg.data_len,
            expected_len,
        ));
    }

    let mut result = Vec::with_capacity(n);
    let dest_ptr = result.as_mut_ptr();
    let src_ptr = msg.data_ptr as *const T;

    unsafe { 
        std::ptr::copy_nonoverlapping(src_ptr, dest_ptr, n);
        result.set_len(n) 
    };

    Ok(result)
}

unsafe fn from_msg_in_single<T>(
    msg: &RawControlMessageIn
) -> Result<T, NetworkError> where T: Sized + Copy {
    let data_len = std::mem::size_of::<T>();
    if msg.data_len != data_len {
        return Err(NetworkError::InvalidControlMessageDataSize(
            msg.data_len,
            data_len,
        ));
    }

    let data_ptr = msg.data_ptr as *const T;
    Ok(unsafe { read_unaligned(data_ptr) })
}
    

trait ParseControlMessage: Sized {
    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, NetworkError>;
}

trait PrepareControlMessage: Sized {
    fn prepare_control_message(self) -> Result<RawControlMessageOut, NetworkError>;
}
    

#[derive(Debug, Clone)]
pub enum IoControlMessageLevel {
    Socket(SocketLevelType),
    IpV4(IpV4LevelType),
    IpV6(IpV6LevelType),
    Tls(TlsLevelType),
}

impl PrepareControlMessage for IoControlMessageLevel {
    fn prepare_control_message(self) -> Result<RawControlMessageOut, NetworkError> {
        match self {
            IoControlMessageLevel::Socket(level) => level.prepare_control_message(),
            IoControlMessageLevel::IpV4(level) => level.prepare_control_message(),
            IoControlMessageLevel::IpV6(level) => level.prepare_control_message(),
            IoControlMessageLevel::Tls(level) => level.prepare_control_message(),
        }
    }
}

impl ParseControlMessage for IoControlMessageLevel {
    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, NetworkError> {
        match cmsg.message_level {
            libc::SOL_SOCKET => {
                Ok(IoControlMessageLevel::Socket(
                    SocketLevelType::parse_control_message(cmsg)?,
                ))
            },
            libc::IPPROTO_IP => {
                Ok(IoControlMessageLevel::IpV4(
                    IpV4LevelType::parse_control_message(cmsg)?,
                ))
            },
            libc::IPPROTO_IPV6 => {
                Ok(IoControlMessageLevel::IpV6(
                    IpV6LevelType::parse_control_message(cmsg)?,
                ))
            },
            libc::SOL_TLS => {
                Ok(IoControlMessageLevel::Tls(
                    TlsLevelType::parse_control_message(cmsg)?,
                ))
            },
            _ => {
                return Err(NetworkError::InvalidControlMessageLevel(
                    cmsg.message_level,
                ));
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum SocketLevelType {
    Rights(Vec<RawFd>),
    Credentials(Credentials),
    Timestamp(std::time::SystemTime),
    DetailedTimestamp {
        software: std::time::SystemTime,
        hardware: std::time::SystemTime,
    },
}

impl ParseControlMessage for SocketLevelType {
    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, NetworkError> {

        match cmsg.message_type {
            libc::SCM_RIGHTS => {
                let fds_slice = unsafe {
                    from_msg_in_multi::<RawFd>(
                        &cmsg,
                        (cmsg.data_len / std::mem::size_of::<RawFd>()) as usize,
                    )?
                };
                let fds = fds_slice.to_vec();

                Ok(SocketLevelType::Rights(fds))
            },
            libc::SCM_CREDENTIALS => {
                let creds = unsafe {
                    from_msg_in_single::<libc::ucred>(&cmsg)?
                };
                Ok(SocketLevelType::Credentials(creds.into()))
            },
            libc::SCM_TIMESTAMP => {
            
                let tv = unsafe {
                    from_msg_in_single::<libc::timeval>(&cmsg)?
                };

                let timestamp = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(
                    tv.tv_sec as u64,
                    tv.tv_usec as u32 * 1000,
                );

                Ok(SocketLevelType::Timestamp(timestamp))
              
            },
            libc::SCM_TIMESTAMPNS => {

                let ts = unsafe { 
                    from_msg_in_single::<libc::timespec>(&cmsg)?
                };

                let timestamp = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(
                    ts.tv_sec as u64,
                    ts.tv_nsec as u32,
                );

                Ok(SocketLevelType::Timestamp(timestamp))
            },
            libc::SCM_TIMESTAMPING => {
                let ts = unsafe {
                    from_msg_in_multi::<libc::timespec>(
                        &cmsg,
                        (cmsg.data_len / std::mem::size_of::<libc::timespec>()) as usize,
                    )?
                };

                if ts.len() != 3 {
                    return Err(NetworkError::InvalidControlMessageDataSize(
                        cmsg.data_len,
                        std::mem::size_of::<libc::timespec>() * 3,
                    ));
                }

                let software = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(
                    ts[0].tv_sec as u64,
                    ts[0].tv_nsec as u32,
                );

                let hardware = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(
                    ts[2].tv_sec as u64,
                    ts[2].tv_nsec as u32,
                );

                Ok(SocketLevelType::DetailedTimestamp {
                    software,
                    hardware,
                })
            },
            _ => {
                return Err(NetworkError::InvalidControlMessageTypeForLevel(
                    cmsg.message_type,
                    libc::SOL_SOCKET,
                ));
            }
        }
    }
}

impl PrepareControlMessage for SocketLevelType {
    fn prepare_control_message(self) -> Result<RawControlMessageOut, NetworkError> {
        let mut msg = RawControlMessageOut {
            message_level: libc::SOL_SOCKET,
            message_type: 0,
            payload: Vec::new(),
        };
        match self {
            SocketLevelType::Rights(fds) => {
                unsafe {
                    fill_msg_out_multi(
                        &mut msg,
                        libc::SCM_RIGHTS,
                        &fds,
                    );
                }
            },
            SocketLevelType::Credentials(creds) => {
                unsafe {
                    fill_msg_out_single::<libc::ucred>(
                        &mut msg,
                        libc::SCM_CREDENTIALS,
                        &creds.into(),
                    );
                }
            },
            SocketLevelType::Timestamp(timestamp) => {
                unsafe {
                    fill_msg_out_single(
                        &mut msg,
                        libc::SCM_TIMESTAMP,
                        &libc::timeval {
                            tv_sec: timestamp.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64,
                            tv_usec: timestamp.duration_since(std::time::UNIX_EPOCH).unwrap().subsec_micros() as i64,
                        },
                    );
                }
            },
            // DetailedTimestamp is not sendable
            SocketLevelType::DetailedTimestamp { .. } => {
                return Err(NetworkError::InvalidControlMessageTypeForLevel(
                    libc::SCM_TIMESTAMPING,
                    libc::SOL_SOCKET,
                ));
            } 
        }
        Ok(msg)
    }
}
            
                

#[derive(Debug, Clone)]
pub enum IpV4LevelType {
    PacketInfo(PacketInfo),
    TimeToLive(i32),
}

impl ParseControlMessage for IpV4LevelType {
    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, NetworkError> {

        match cmsg.message_type {
            libc::IP_PKTINFO => {
                let pktinfo = unsafe { 
                    from_msg_in_single::<libc::in_pktinfo>(&cmsg)?
                };
                Ok(IpV4LevelType::PacketInfo(pktinfo.into()))
            },
            libc::IP_TTL => {
                let ttl = unsafe {
                    from_msg_in_single::<i32>(&cmsg)?
                };
                Ok(IpV4LevelType::TimeToLive(ttl))
            },
            libc::IP_RECVERR => {
                let err = unsafe {
                    from_msg_in_single::<libc::sock_extended_err>(&cmsg)?
                };
                Err(NetworkError::ExtendedSocketError(err.into()))
            },
            _ => {
                return Err(NetworkError::InvalidControlMessageTypeForLevel(
                    cmsg.message_type,
                    libc::IPPROTO_IP,
                ));
            }
        }
    }
}

impl PrepareControlMessage for IpV4LevelType {
    fn prepare_control_message(self) -> Result<RawControlMessageOut, NetworkError> {
        let mut msg = RawControlMessageOut {
            message_level: libc::IPPROTO_IP,
            message_type: 0,
            payload: Vec::new(),
        };
        match self {
            IpV4LevelType::PacketInfo(pktinfo) => {
                let libc_pktinfo: LibCPktInfo = pktinfo.try_into()?;
                match libc_pktinfo {
                    LibCPktInfo::V4(pktinfo) => {
                        unsafe {
                            fill_msg_out_single(
                                &mut msg,
                                libc::IP_PKTINFO,
                                &pktinfo,
                            );
                        }
                    },
                    LibCPktInfo::V6(_) => {
                        // This should not happen, as we are only dealing with IPv4 here
                        return Err(NetworkError::InvalidControlMessageTypeForLevel(
                            libc::IP_PKTINFO,
                            libc::IPPROTO_IP,
                        ));
                    }
                }
            },
            IpV4LevelType::TimeToLive(ttl) => {
                unsafe {
                    fill_msg_out_single(
                        &mut msg,
                        libc::IP_TTL,
                        &ttl,
                    );
                }
            },
        }
        Ok(msg)
    }
}

#[derive(Debug, Clone)]
pub enum IpV6LevelType {
    PacketInfo(PacketInfo),
    HopLimit(i32),
    Class(IpV6TrafficClass),
}

impl ParseControlMessage for IpV6LevelType {
    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, NetworkError> {

        match cmsg.message_type {
            libc::IPV6_PKTINFO => {
                let pktinfo = unsafe {
                    from_msg_in_single::<libc::in6_pktinfo>(&cmsg)?
                };
                Ok(IpV6LevelType::PacketInfo(pktinfo.into()))
            },
            libc::IPV6_HOPLIMIT => {
                let hops = unsafe {
                    from_msg_in_single::<i32>(&cmsg)?
                };
                Ok(IpV6LevelType::HopLimit(hops))
            },
            libc::IPV6_TCLASS => {
                let class = unsafe {
                    from_msg_in_single::<i32>(&cmsg)?
                };
                Ok(IpV6LevelType::Class((class as u8).into()))
            },
            libc::IPV6_RECVERR => {
                let err = unsafe {
                    from_msg_in_single::<libc::sock_extended_err>(&cmsg)?
                };
                Err(NetworkError::ExtendedSocketError(err.into()))
            },
            _ => {
                return Err(NetworkError::InvalidControlMessageTypeForLevel(
                    cmsg.message_type,
                    libc::IPPROTO_IPV6,
                ));
            }
        }
    }
}

impl PrepareControlMessage for IpV6LevelType {
    fn prepare_control_message(self) -> Result<RawControlMessageOut, NetworkError> {
        let mut msg = RawControlMessageOut {
            message_level: libc::IPPROTO_IPV6,
            message_type: 0,
            payload: Vec::new(),
        };
        match self {
            IpV6LevelType::PacketInfo(pktinfo) => {
                let libc_pktinfo: LibCPktInfo = pktinfo.try_into()?;
                match libc_pktinfo {
                    LibCPktInfo::V4(_) => {
                        // This should not happen, as we are only dealing with IPv6 here
                        return Err(NetworkError::InvalidControlMessageTypeForLevel(
                            libc::IPV6_PKTINFO,
                            libc::IPPROTO_IPV6,
                        ));
                    },
                    LibCPktInfo::V6(pktinfo) => {
                        unsafe {
                            fill_msg_out_single(
                                &mut msg,
                                libc::IPV6_PKTINFO,
                                &pktinfo,
                            );
                        }
                    }
                }
            },
            IpV6LevelType::HopLimit(hops) => {
                unsafe {
                    fill_msg_out_single(
                        &mut msg,
                        libc::IPV6_HOPLIMIT,
                        &hops,
                    );
                }
            },
            IpV6LevelType::Class(class) => {
                let class: u8 = class.into();
                unsafe {
                    fill_msg_out_single::<libc::c_int>(
                        &mut msg,
                        libc::IPV6_TCLASS,
                        &(class as libc::c_int),
                    );
                }
            }
        }
        Ok(msg)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct IpV6TrafficClass {
    pub explicit_congestion_notification: IpV6Ecn,
    pub differentiated_services_code_point: IpV6Dscp,
}

impl From<u8> for IpV6TrafficClass {
    fn from(byte: u8) -> Self {

        let ecn_raw = byte & 0b11;

        let ecn = match ecn_raw {
            0 => IpV6Ecn::NotEct,
            1 => IpV6Ecn::Ect1,
            2 => IpV6Ecn::Ect0,
            _ => IpV6Ecn::Ce,
        };

        let dscp_raw = (byte & 0b11111100) >> 2;

        let dscp = match dscp_raw {
            0 => IpV6Dscp::Default,
            8 => IpV6Dscp::ClassSelector1,
            16 => IpV6Dscp::ClassSelector2,
            24 => IpV6Dscp::ClassSelector3,
            32 => IpV6Dscp::ClassSelector4,
            40 => IpV6Dscp::ClassSelector5,
            48 => IpV6Dscp::ClassSelector6,
            56 => IpV6Dscp::ClassSelector7,
            46 => IpV6Dscp::ExpeditedForwarding,
            10 => IpV6Dscp::AssuredForwarding11,
            12 => IpV6Dscp::AssuredForwarding12,
            14 => IpV6Dscp::AssuredForwarding13,
            18 => IpV6Dscp::AssuredForwarding21,
            20 => IpV6Dscp::AssuredForwarding22,
            22 => IpV6Dscp::AssuredForwarding23,
            26 => IpV6Dscp::AssuredForwarding31,
            28 => IpV6Dscp::AssuredForwarding32,
            30 => IpV6Dscp::AssuredForwarding33,
            34 => IpV6Dscp::AssuredForwarding41,
            36 => IpV6Dscp::AssuredForwarding42,
            38 => IpV6Dscp::AssuredForwarding43,
            _ => IpV6Dscp::Other(dscp_raw),
        };

        Self {
            explicit_congestion_notification: ecn,
            differentiated_services_code_point: dscp,
        }
    }
}

impl Into<u8> for IpV6TrafficClass {
    fn into(self) -> u8 {
        let ecn = match self.explicit_congestion_notification {
            IpV6Ecn::NotEct => 0,
            IpV6Ecn::Ect0 => 2,
            IpV6Ecn::Ect1 => 1,
            IpV6Ecn::Ce => 3,
        };

        let dscp = match self.differentiated_services_code_point {
            IpV6Dscp::Default => 0,
            IpV6Dscp::ClassSelector1 => 8,
            IpV6Dscp::ClassSelector2 => 16,
            IpV6Dscp::ClassSelector3 => 24,
            IpV6Dscp::ClassSelector4 => 32,
            IpV6Dscp::ClassSelector5 => 40,
            IpV6Dscp::ClassSelector6 => 48,
            IpV6Dscp::ClassSelector7 => 56,
            IpV6Dscp::ExpeditedForwarding => 46,
            IpV6Dscp::AssuredForwarding11 => 10,
            IpV6Dscp::AssuredForwarding12 => 12,
            IpV6Dscp::AssuredForwarding13 => 14,
            IpV6Dscp::AssuredForwarding21 => 18,
            IpV6Dscp::AssuredForwarding22 => 20,
            IpV6Dscp::AssuredForwarding23 => 22,
            IpV6Dscp::AssuredForwarding31 => 26,
            IpV6Dscp::AssuredForwarding32 => 28,
            IpV6Dscp::AssuredForwarding33 => 30,
            IpV6Dscp::AssuredForwarding41 => 34,
            IpV6Dscp::AssuredForwarding42 => 36,
            IpV6Dscp::AssuredForwarding43 => 38,
            IpV6Dscp::Other(value) => value,
        };

        (dscp << 2) | ecn
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IpV6Ecn {
    NotEct,
    Ect0,
    Ect1,
    Ce,
}

#[derive(Debug, Clone, Copy)]
pub enum IpV6Dscp {
    Default,
    ClassSelector1,
    ClassSelector2,
    ClassSelector3,
    ClassSelector4,
    ClassSelector5,
    ClassSelector6,
    ClassSelector7,
    ExpeditedForwarding,
    AssuredForwarding11,
    AssuredForwarding12,
    AssuredForwarding13,
    AssuredForwarding21,
    AssuredForwarding22,
    AssuredForwarding23,
    AssuredForwarding31,
    AssuredForwarding32,
    AssuredForwarding33,
    AssuredForwarding41,
    AssuredForwarding42,
    AssuredForwarding43,
    Other(u8),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
pub enum TlsRecordType {
    ChangeCipherSpec    = 20,
    Alert               = 21,
    Handshake           = 22,
    ApplicationData     = 23,
    Heartbeat           = 24
}

#[derive(Debug, Clone)]
pub enum TlsLevelType {
    SetRecordType(TlsRecordType),
    GetRecordType(TlsRecordType)
}

impl ParseControlMessage for TlsLevelType {
    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, NetworkError> {

        match cmsg.message_type {
            libc::TLS_SET_RECORD_TYPE => {
                let record_type = unsafe {
                    from_msg_in_single::<TlsRecordType>(&cmsg)?
                };
                Ok(TlsLevelType::SetRecordType(record_type))
            },
            libc::TLS_GET_RECORD_TYPE => {
                let record_type = unsafe {
                    from_msg_in_single::<TlsRecordType>(&cmsg)?
                };
                Ok(TlsLevelType::GetRecordType(record_type))
            },
            _ => {
                return Err(NetworkError::InvalidControlMessageTypeForLevel(
                    cmsg.message_type,
                    libc::SOL_TLS,
                ));
            }
        }
    }
}

impl PrepareControlMessage for TlsLevelType {
    fn prepare_control_message(self) -> Result<RawControlMessageOut, NetworkError> {
        let mut msg = RawControlMessageOut {
            message_level: libc::SOL_TLS,
            message_type: 0,
            payload: Vec::new(),
        };
        match self {
            TlsLevelType::SetRecordType(record_type) => {
                unsafe {
                    fill_msg_out_single(
                        &mut msg,
                        libc::TLS_SET_RECORD_TYPE,
                        &record_type,
                    );
                }
            },
            TlsLevelType::GetRecordType(_) => {
                return Err(NetworkError::InvalidControlMessageTypeForLevel(
                    libc::TLS_GET_RECORD_TYPE,
                    libc::SOL_TLS,
                ));
            },
        }
        Ok(msg)
    }
}

#[derive(Debug, Clone)]
pub struct IoControlMessage {
    level: IoControlMessageLevel,
}

impl IoControlMessage {

    pub fn new(level: IoControlMessageLevel) -> Self { Self { level } }

    pub fn level(&self) -> &IoControlMessageLevel { &self.level }
    pub fn level_mut(&mut self) -> &mut IoControlMessageLevel { &mut self.level }

    pub fn try_parse_single(cmsg_ptr: *const libc::cmsghdr) -> Result<Self, NetworkError> {

        if cmsg_ptr.is_null() {
            return Err(NetworkError::NullPointerError);
        }

        let cmsg_val = unsafe { *cmsg_ptr };

        let base_size = unsafe { libc::CMSG_LEN(0) as usize };
        let hdr_size = cmsg_val.cmsg_len as usize;

        if hdr_size < base_size {
            return Err(NetworkError::InvalidControlMessageSize(hdr_size));
        }

        let raw_control_message = RawControlMessageIn {
            message_level: cmsg_val.cmsg_level,
            message_type: cmsg_val.cmsg_type,
            data_ptr: unsafe { libc::CMSG_DATA(cmsg_ptr as *const _) as *const libc::c_uchar },
            data_len: hdr_size - base_size,
            _lifetime: std::marker::PhantomData,
        };

        Ok(Self { 
            level: IoControlMessageLevel::parse_control_message(raw_control_message)?,
        })
    }

    pub unsafe fn try_parse_from_slice(cmsgs: &[u8]) -> Result<Vec<Self>, NetworkError> {

        if cmsgs.is_empty() { return Ok(vec![]); }

        let mut parsed_messages = Vec::new();
        let mut offset = 0;

        let base_len = unsafe { libc::CMSG_LEN(0) as usize };

        while offset < cmsgs.len() {
            let cmsg_ptr = unsafe { cmsgs.as_ptr().add(offset) as *const libc::cmsghdr };
            let msg = Self::try_parse_single(cmsg_ptr)?;
            parsed_messages.push(msg);
            
            let len = unsafe { (*cmsg_ptr).cmsg_len as usize };
            let data_len = len - base_len;
            let space = unsafe { libc::CMSG_SPACE(data_len as _) } as usize;
            offset += space;
        }

        Ok(parsed_messages)
    }

    pub fn try_parse_from_msghdr(msghdr: &libc::msghdr) -> Result<Vec<Self>, NetworkError> {

        let cmsg_ptr = unsafe { libc::CMSG_FIRSTHDR(msghdr) };
        if cmsg_ptr.is_null() { return Err(NetworkError::NullPointerError); }

        let mut messages = Vec::new();
        
        let mut current_cmsg_ptr = cmsg_ptr;
        while !current_cmsg_ptr.is_null() {
            let message = IoControlMessage::try_parse_single(current_cmsg_ptr)?;
            messages.push(message);

            current_cmsg_ptr = unsafe { libc::CMSG_NXTHDR(msghdr, current_cmsg_ptr) };
        }

        Ok(messages)
    }

    pub fn try_prepare(self) -> Result<Vec<u8>, NetworkError> {
        let raw_control_message = self.level.prepare_control_message()?;
        let space = unsafe { libc::CMSG_SPACE(raw_control_message.payload.len() as _) } as usize;
        let mut bytes = vec![0u8; space];
        let cmsg_ptr = bytes.as_mut_ptr() as *mut libc::cmsghdr;

        unsafe {
            (*cmsg_ptr).cmsg_level = raw_control_message.message_level;
            (*cmsg_ptr).cmsg_type = raw_control_message.message_type;
            (*cmsg_ptr).cmsg_len = libc::CMSG_LEN(raw_control_message.payload.len() as _) as _;
        }

        let data_ptr = unsafe { libc::CMSG_DATA(cmsg_ptr as *const _) as *mut libc::c_uchar };
        unsafe {
            std::ptr::copy_nonoverlapping(
                raw_control_message.payload.as_ptr(),
                data_ptr,
                raw_control_message.payload.len(),
            );
        }

        Ok(bytes)
    }

    pub fn try_prepare_multi(msgs: Vec<IoControlMessage>) -> Result<Vec<u8>, NetworkError> {

        use PrepareControlMessage;

        let mut prepared_messages = Vec::with_capacity(msgs.len());
        let mut total_size = 0;
        for msg in msgs {
            let raw_control_message = msg.level.prepare_control_message()?;
            let space = unsafe { libc::CMSG_SPACE(raw_control_message.payload.len() as _) } as usize;
            total_size += space;
            prepared_messages.push((raw_control_message, space));
        }

        let mut bytes = vec![0u8; total_size];
        let mut offset = 0;
        for i in 0..prepared_messages.len() {
            let (msg, space) = &prepared_messages[i];
            let len = unsafe { libc::CMSG_LEN(msg.payload.len() as _) } as usize;
            let cmsg_ptr = unsafe { bytes.as_mut_ptr().add(offset) as *mut libc::cmsghdr };

            unsafe {
                (*cmsg_ptr).cmsg_level = msg.message_level;
                (*cmsg_ptr).cmsg_type = msg.message_type;
                (*cmsg_ptr).cmsg_len = len as _;
            }

            let data_ptr = unsafe { libc::CMSG_DATA(cmsg_ptr as *const _) as *mut libc::c_uchar };

            unsafe {
                std::ptr::copy_nonoverlapping(
                    msg.payload.as_ptr(),
                    data_ptr,
                    msg.payload.len(),
                );
            }

            offset += space;
        }

        Ok(bytes)
    }
}

pub struct IoMessage {
    control: Option<Vec<IoControlMessage>>,
    address: Option<PeerAddress>,
    data: Option<Vec<Vec<u8>>>,
    flags: Option<IoRecvMsgOutputFlags>,

    _control_buffer: Option<Vec<u8>>,
}

impl IoMessage {

    pub fn new(
        control: Option<Vec<IoControlMessage>>,
        address: Option<PeerAddress>,
        buffers: Option<Vec<Vec<u8>>>,
    ) -> Self {
        Self {
            control,
            address,
            data: buffers,
            flags: None,
            _control_buffer: None,
        }
    }

    // Common canned messages

    // TLS close_notify
    pub fn close_notify() -> Self {

        Self::default()

        .add_control_message(IoControlMessage::new(
            IoControlMessageLevel::Tls(TlsLevelType::SetRecordType(
                TlsRecordType::Alert,
            )),
        ))
        
        .add_data_buffer(vec![
            TlsAlertLevel::Warning as u8,
            TlsAlertDescription::CloseNotify as u8,
        ])
    }

    // Send simple buffer with no control
    pub fn send_buffer(buffer: IoInputBuffer) -> Self {

        Self::default()
            
        .add_data_buffer(buffer.into_vec())
    }

    // Send multiple buffers with no control
    pub fn send_buffers(buffers: Vec<IoInputBuffer>) -> Self {

        let mut ret = Self::default();
            
        for buffer in buffers {
            ret = ret.add_data_buffer(buffer.into_vec());
        }

        ret
    }

    // Send simple buffer to sepcific address with no control
    pub fn send_buffer_to(
        buffer: IoInputBuffer,
        address: PeerAddress,
    ) -> Self {

        Self::default()
            
        .set_address(address)
        .add_data_buffer(buffer.into_vec())
    }

    // Send multiple buffers to sepcific address with no control
    pub fn send_buffers_to(
        buffers: Vec<IoInputBuffer>,
        address: PeerAddress,
    ) -> Self {

        let mut ret = Self::default().set_address(address);
        
        for buffer in buffers {
            ret = ret.add_data_buffer(buffer.into_vec());
        }

        ret
    }

    // Send fds on on Unix domain sockets
    pub fn send_fds(fds: Vec<RawFd>) -> Self {

        Self::default()
            
        .add_control_message(IoControlMessage::new(
            IoControlMessageLevel::Socket(
                SocketLevelType::Rights(fds)
            ),
        ))
    }

    // Send credentials for authentication
    pub fn send_credentials(creds: Credentials) -> Self {

        Self::default()
            
        .add_control_message(IoControlMessage::new(
            IoControlMessageLevel::Socket(
                SocketLevelType::Credentials(creds)
            ),
        ))
    }

    pub fn add_control_message(
        mut self,
        message: IoControlMessage,
    ) -> Self {

        if self.control.is_none() {
            self.control = Some(Vec::new());
        }

        if let Some(control) = &mut self.control {
            control.push(message);
        }

        self
    }

    pub fn add_control_messages(
        mut self,
        messages: Vec<IoControlMessage>,
    ) -> Self {

        if self.control.is_none() {
            self.control = Some(Vec::new());
        }

        if let Some(control) = &mut self.control {
            control.extend(messages);
        }

        self
    }

    pub fn clear_control_messages(mut self) -> Self {
        self.control = None;
        self
    }

    pub fn set_address(mut self, address: PeerAddress) -> Self {
        self.address = Some(address);
        self
    }

    pub fn clear_address(mut self) -> Self {
        self.address = None;
        self
    }

    pub fn add_data_buffer(
        mut self,
        buffer: Vec<u8>,
    ) -> Self {

        if self.data.is_none() {
            self.data = Some(Vec::new());
        }

        if let Some(buffers) = &mut self.data {
            buffers.push(buffer);
        }

        self
    }

    pub fn add_data_buffers(
        mut self,
        buffers: Vec<Vec<u8>>,
    ) -> Self {
        if self.data.is_none() {
            self.data = Some(Vec::new());
        }

        if let Some(existing_buffers) = &mut self.data {
            existing_buffers.extend(buffers);
        }

        self
    }

    pub fn clear(mut self) -> Self {
        self.data = None;
        self
    }

    pub fn take_control_messages(&mut self) -> Option<Vec<IoControlMessage>> {
        self.control.take()
    }

    pub fn take_control_buffer(&mut self) -> Option<Vec<u8>> {
        self._control_buffer.take()
    }

    pub fn take_address(&mut self) -> Option<PeerAddress> {
        self.address.take()
    }

    pub fn take_buffers(&mut self) -> Option<Vec<Vec<u8>>> {
        self.data.take()
    }

    pub fn control_messages(&self) -> Option<&Vec<IoControlMessage>> {
        self.control.as_ref()
    }

    pub fn address(&self) -> Option<&PeerAddress> {
        self.address.as_ref()
    }

    pub fn buffers(&self) -> Option<&Vec<Vec<u8>>> {
        self.data.as_ref()
    }

    pub fn control_buffer(&self) -> Option<&Vec<u8>> {
        self._control_buffer.as_ref()
    }

    pub fn flags(&self) -> Option<IoRecvMsgOutputFlags> { self.flags }
    pub fn take_flags(&mut self) -> Option<IoRecvMsgOutputFlags> { self.flags.take() }

    pub fn prepare(self) -> Result<PreparedIoMessage<IoDoubleInputBuffer>, IoOperationError> {

        let control = match self.control {
            Some(msgs) => {
                Some(IoControlMessage::try_prepare_multi(msgs)?)
            },
            None => None,
        };

        let address = match self.address {
            Some(addr) => PreparedIoMessageAddressDirection::Input(addr),
            None => PreparedIoMessageAddressDirection::None,
        };

        let buffers =  match self.data {
            Some(buffers) => {
                Some(IoDoubleInputBuffer::try_from(buffers)?)
            },
            None => None,
        };

        Ok(PreparedIoMessage::new(
            buffers, 
            control, 
            address,
        )?)
    }

    pub fn is_finished(&self) -> bool {
        self.data.is_some() && self.data.as_ref().unwrap().is_empty()
    }

    pub fn get_alert(&self) -> Option<TlsAlert> {
        if let Some(control) = &self.control {
            for msg in control {
                if let IoControlMessageLevel::Tls(TlsLevelType::GetRecordType(record_type)) = msg.level {
                    if record_type == TlsRecordType::Alert {
                        if let Some(buffers) = &self.data {
                            if buffers.len() == 1 && buffers[0].len() <= 2 {
                                return Some(TlsAlert {
                                    level: buffers[0][0].into(),
                                    description: buffers[0].get(1).map(|&b| b.into())
                                });
                            }
                        }
                    }
                }
            }
        }
        None
    }
    
}

pub enum PreparedIoMessageAddressDirection {
    None,
    Input(PeerAddress),
    Output,
}

pub struct PreparedIoMessageInner<T: IoDoubleBuffer> {
    buffers: Option<T>, // Application data buffers
    control: Option<Vec<u8>>, // Control buffer
    address: Option<libc::sockaddr_storage>, // Address storage

    // Pointer structures
    _iovec: Option<Vec<libc::iovec>>,
    _msghdr: libc::msghdr,
}

impl<T: IoDoubleBuffer> PreparedIoMessageInner<T> {
    pub fn new(
        mut buffers: Option<T>,
        control: Option<Vec<u8>>,
        address: PreparedIoMessageAddressDirection,
    ) -> Result<Pin<Box<Self>>, IoOperationError> {

        let _iovec = match buffers.as_mut() {
            None => None,
            Some(buffers) => {
                Some(unsafe { buffers.generate_iovecs()? })
            }
        };

        let (_name_len, _name) = match address {
            PreparedIoMessageAddressDirection::Input(addr) => {
                let len = match addr.ip().version() {
                    IpVersion::V4 => {
                        std::mem::size_of::<libc::sockaddr_in>() as u32
                    },
                    IpVersion::V6 => {
                        std::mem::size_of::<libc::sockaddr_in6>() as u32
                    }
                };
                (len, Some(addr.try_into()?))
            },
            PreparedIoMessageAddressDirection::Output => {(
                    std::mem::size_of::<libc::sockaddr_storage>() as u32,
                    Some(unsafe { std::mem::zeroed() })
            )},
            PreparedIoMessageAddressDirection::None => (0, None),
        };

        let data = Self {
            buffers,
            control,
            address: _name,
            _iovec,
            _msghdr: unsafe { std::mem::zeroed() },
        };

        let mut pinned_self = Box::pin(data);

        unsafe {
            let data_mut = Pin::get_unchecked_mut(pinned_self.as_mut());
            let msghdr = &mut data_mut._msghdr;

            msghdr.msg_name = data_mut.address.as_ref().map_or(std::ptr::null_mut(), |addr| addr as *const _ as *mut _);
            msghdr.msg_namelen = _name_len;

            msghdr.msg_iov = data_mut._iovec.as_ref().map_or(std::ptr::null_mut(), |iov| iov.as_ptr() as *mut _);
            msghdr.msg_iovlen = data_mut._iovec.as_ref().map_or(0, |iov| iov.len() as _);

            msghdr.msg_control = data_mut.control.as_ref().map_or(std::ptr::null_mut(), |control| control.as_ptr() as *mut _);
            msghdr.msg_controllen = data_mut.control.as_ref().map_or(0, |control| control.len() as _);
        }

        Ok(pinned_self)
    }
}

pub struct PreparedIoMessage<T: IoDoubleBuffer> {
    // Using pin because PreparedIoMessageInner is self-referential
    inner: Pin<Box<PreparedIoMessageInner<T>>>,
}

impl<T: IoDoubleBuffer> PreparedIoMessage<T> {

    pub fn new(
        buffers: Option<T>,
        control: Option<Vec<u8>>,
        address: PreparedIoMessageAddressDirection
    ) -> Result<Self, IoOperationError> {

        Ok(Self {
            inner: PreparedIoMessageInner::new(
                buffers,
                control,
                address,
            )?,
        })
    }
}

impl PreparedIoMessage<IoDoubleOutputBuffer> {

    pub fn into_message(mut self, bytes_received: usize) -> Result<IoMessage, IoOperationError> {
        
        let addr_len = self.inner._msghdr.msg_namelen as usize;

        let address = if addr_len == 0 { None } 
        else { 
            match self.inner.address {
                Some(addr) => Some(PeerAddress::try_from(addr)?),
                None => None,
            }
        };

        let buffers = self.inner.buffers.take();

        let data = match buffers {
            None => None,
            Some(buffers) => {
                Some(buffers.into_vec(bytes_received)?)
            }
        };

        let flags = IoRecvMsgOutputFlags::from_bits(self.inner._msghdr.msg_flags);

        let control = match IoControlMessage::try_parse_from_msghdr(
            &self.inner._msghdr,
        ) {
            Ok(control) => Some(control),
            Err(NetworkError::NullPointerError) => None,
            Err(err) => return Err(IoOperationError::from(err))
        };

        let _control_buffer = self.inner.control.take();

        Ok(IoMessage {
            control,
            address,
            data,
            flags,
            _control_buffer,
        })
    }
}

impl<T: IoDoubleBuffer> AsRef<libc::msghdr> for PreparedIoMessage<T> {
    fn as_ref(&self) -> &libc::msghdr {
        &self.inner._msghdr
    }
}

impl<T: IoDoubleBuffer> AsMut<libc::msghdr> for PreparedIoMessage<T> {
    fn as_mut(&mut self) -> &mut libc::msghdr {
        unsafe { &mut Pin::get_unchecked_mut(self.inner.as_mut())._msghdr }
    }
}

/* UNSOUND

    * This is unsafe because the structure contains raw pointer.s
    * Use of this type should be done with EXTREME caution and care
    * This type is only used in strictly regulated code paths

*/
unsafe impl<T: IoDoubleBuffer> Send for PreparedIoMessage<T> {}

impl Default for IoMessage {

    fn default() -> Self {
        Self {
            control: None,
            address: None,
            data: None,
            flags: None,
            _control_buffer: None,
        }
    }
}

impl TryFromCompletion for IoMessage {

    fn try_from_completion(
        completion: IoCompletion
    ) -> Result<Self, IoError> {
        match completion {
            IoCompletion::Msg(completion) => {
                Ok(completion.msg)
            },
            _ => {
                Err(IoOperationError::UnexpectedPollResult(
                    String::from("Message future got unexpected poll result")
                ).into())
            }
        }
    }
}

pub type IoMessageFuture = IoOperationFuture<IoMessage>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::RawFd;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Helper to round-trip a single control message
    fn round_trip_single(msg: IoControlMessage) -> IoControlMessage {
        let buf = msg.clone().try_prepare().expect("prepare failed");
        let parsed = unsafe { IoControlMessage::try_parse_from_slice(&buf) }.expect("parse failed");
        assert_eq!(parsed.len(), 1);
        parsed.into_iter().next().unwrap()
    }

    #[test]
    fn test_rights_round_trip() {
        let fds = vec![3 as RawFd, 4 as RawFd, 5 as RawFd];
        let msg = IoControlMessage::new(IoControlMessageLevel::Socket(
            SocketLevelType::Rights(fds.clone()),
        ));
        let parsed = round_trip_single(msg);

        if let IoControlMessageLevel::Socket(SocketLevelType::Rights(parsed_fds)) = parsed.level() {
            assert_eq!(parsed_fds, &fds);
        } else {
            panic!("Parsed message is not Rights");
        }
    }

    #[test]
    fn test_credentials_round_trip() {
        let creds = Credentials {
            process_id: 1234,
            user_id: 1000,
            group_id: 1000,
        };
        let msg = IoControlMessage::new(IoControlMessageLevel::Socket(
            SocketLevelType::Credentials(creds),
        ));
        let parsed = round_trip_single(msg);

        if let IoControlMessageLevel::Socket(SocketLevelType::Credentials(parsed_creds)) = parsed.level() {
            assert_eq!(parsed_creds.process_id, creds.process_id);
            assert_eq!(parsed_creds.user_id, creds.user_id);
            assert_eq!(parsed_creds.group_id, creds.group_id);
        } else {
            panic!("Parsed message is not Credentials");
        }
    }

    #[test]
    fn test_pktinfo_v4_round_trip() {
        let pktinfo = PacketInfo {
            interface_index: 2,
            destination: IpAddress::V4([192, 168, 1, 1]),
            specific_destination: Some(IpAddress::V4([192, 168, 1, 100])),
        };
        let msg = IoControlMessage::new(IoControlMessageLevel::IpV4(
            IpV4LevelType::PacketInfo(pktinfo),
        ));
        let parsed = round_trip_single(msg);

        if let IoControlMessageLevel::IpV4(IpV4LevelType::PacketInfo(parsed_pktinfo)) = parsed.level() {
            assert_eq!(parsed_pktinfo.interface_index, 2);
            assert_eq!(parsed_pktinfo.destination, IpAddress::V4([192, 168, 1, 1]));
            assert_eq!(parsed_pktinfo.specific_destination, Some(IpAddress::V4([192, 168, 1, 100])));
        } else {
            panic!("Parsed message is not PacketInfo");
        }
    }

    #[test]
    fn test_tls_record_type_round_trip() {
        let msg = IoControlMessage::new(IoControlMessageLevel::Tls(
            TlsLevelType::SetRecordType(TlsRecordType::ApplicationData),
        ));
        let parsed = round_trip_single(msg);

        if let IoControlMessageLevel::Tls(TlsLevelType::SetRecordType(rt)) = parsed.level() {
            assert_eq!(*rt, TlsRecordType::ApplicationData);
        } else {
            panic!("Parsed message is not Tls SetRecordType");
        }
    }

    #[test]
    fn test_timestamp_round_trip() {
        let now = SystemTime::now();
        let msg = IoControlMessage::new(IoControlMessageLevel::Socket(
            SocketLevelType::Timestamp(now),
        ));
        let parsed = round_trip_single(msg);

        if let IoControlMessageLevel::Socket(SocketLevelType::Timestamp(parsed_time)) = parsed.level() {
            // Allow a small difference due to conversion precision
            let orig = now.duration_since(UNIX_EPOCH).unwrap().as_secs();
            let parsed = parsed_time.duration_since(UNIX_EPOCH).unwrap().as_secs();
            assert!((orig as i64 - parsed as i64).abs() <= 1);
        } else {
            panic!("Parsed message is not Timestamp");
        }
    }

    #[test]
    fn test_rights_alignment() {
        // Use u64 to test alignment handling
        let data = vec![0x1122334455667788u64, 0x99aabbccddeeff00u64];
        let mut msg = RawControlMessageOut {
            message_level: libc::SOL_SOCKET,
            message_type: libc::SCM_RIGHTS,
            payload: Vec::new(),
        };
        unsafe {
            fill_msg_out_multi(&mut msg, libc::SCM_RIGHTS, &data);
        }
        // Now parse it back
        let msg_in = RawControlMessageIn {
            message_level: libc::SOL_SOCKET,
            message_type: libc::SCM_RIGHTS,
            data_len: msg.payload.len(),
            data_ptr: msg.payload.as_ptr(),
            _lifetime: std::marker::PhantomData,
        };
        let parsed: Vec<u64> = unsafe { from_msg_in_multi(&msg_in, data.len()).unwrap() };
        assert_eq!(parsed, data);
    }

    #[test]
    fn test_error_on_size_mismatch() {
        let msg_in = RawControlMessageIn {
            message_level: libc::SOL_SOCKET,
            message_type: libc::SCM_CREDENTIALS,
            data_len: 1, // Intentionally wrong
            data_ptr: [0u8; 1].as_ptr(),
            _lifetime: std::marker::PhantomData,
        };
        let result = unsafe { from_msg_in_single::<libc::ucred>(&msg_in) };
        assert!(result.is_err());
    }
}