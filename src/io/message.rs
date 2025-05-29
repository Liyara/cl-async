use std::{fmt, os::fd::RawFd, pin::Pin, ptr::read_unaligned, sync::Arc};

use bytes::{BufMut, Bytes, BytesMut};
use thiserror::Error;

use crate::{io::buffers::RecvMsgSubmissionError, net::{CSockExtendedError, IpAddress, IpVersion, PeerAddress, UnsupportedAddressFamilyError}};

use super::{buffers::{IoOutputBufferIntoBytesError, IoRecvMsgOutputBuffers, IoVecInputSource, IoVecOutputBuffer, IoVecOutputBufferIntoBytesError, RecvMsgSubmissionErrorKind}, operation_data::IoRecvMsgOutputFlags, GenerateIoVecs, InvalidIoVecError, IoInputBuffer, IoOutputBuffer, IoSubmissionError, RecvMsgBuffers};

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

#[derive(Debug, Error)]
#[error("Failed to convert PacketInfo to LibCPktInfo")]
pub struct PacketInfoConversionError(pub PacketInfo);

impl TryInto<LibCPktInfo> for PacketInfo {
    
    type Error = PacketInfoConversionError;
    
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
                        _ => return Err(PacketInfoConversionError(self)),
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

#[derive(Debug, Error)]
#[error("Failed to extract data from raw message; Expected message with data length {expected}, but got {actual}")]
pub struct FromMessageInError {
    pub expected: usize,
    pub actual: usize
}

unsafe fn from_msg_in_multi<T>(
    msg: &RawControlMessageIn, n: usize
) -> Result<Vec<T>, FromMessageInError> where T: Copy + Sized {
    let expected_len = std::mem::size_of::<T>() * n;
    if msg.data_len != expected_len {
        return Err(FromMessageInError {
            expected: expected_len,
            actual: msg.data_len,
        });
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
) -> Result<T, FromMessageInError> where T: Sized + Copy {
    let data_len = std::mem::size_of::<T>();
    if msg.data_len != data_len {
        return Err(FromMessageInError {
            expected: data_len,
            actual: msg.data_len,
        });
    }

    let data_ptr = msg.data_ptr as *const T;
    Ok(unsafe { read_unaligned(data_ptr) })
}
    

trait ParseControlMessage: Sized {
    type Error;
    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, Self::Error>;
}

trait PrepareControlMessage: Sized {
    type Error;
    fn prepare_control_message(self) -> Result<RawControlMessageOut, Self::Error>;
}
    

#[derive(Debug, Clone)]
pub enum IoControlMessageLevel {
    Socket(SocketLevelType),
    IpV4(IpV4LevelType),
    IpV6(IpV6LevelType),
    Tls(TlsLevelType),
}

#[derive(Debug, Error)]
pub enum ParseControlMessageTypeError {
    #[error("Invalid control message type {message_type} for level {message_level}")]
    InvalidControlMessageTypeForLevel {
        message_type: libc::c_int,
        message_level: libc::c_int,
    }
}

#[derive(Debug, Error)]
pub enum ParseControlMessageSocketError {
    #[error("Failed to retrieve file descriptors: {0}")]
    FailedToRetrieveFileDescriptors(#[source] FromMessageInError),

    #[error("Failed to retrieve credentials: {0}")]
    FailedToRetrieveCredentials(#[source] FromMessageInError),

    #[error("Failed to retrieve timestamp: {0}")]
    FailedToRetrieveTimestamp(#[source] FromMessageInError),

    #[error("Epected {expected} timestamps, but got {actual}")]
    InvalidNumberOfTimestamps {
        expected: usize,
        actual: usize,
    },

    #[error("Invalid control message type {message_type} for level {message_level}")]
    InvalidControlMessageTypeForLevel {
        message_type: libc::c_int,
        message_level: libc::c_int,
    },

    #[error("Error while parsing socket control message: {0}")]
    ParseControlMessageTypeError(#[from] ParseControlMessageTypeError),
}

#[derive(Debug, Error)]
pub enum ParseControlMessageIpV4Error {

    #[error("Failed to retrieve time to live: {0}")]
    FailedToRetrieveTimeToLive(#[source] FromMessageInError),

    #[error("Failed to retrieve packet info: {0}")]
    FailedToRetrievePacketInfo(#[source] FromMessageInError),

    #[error("Failed to retrieve extended error: {0}")]
    FailedToRetrieveExtendedError(#[source] FromMessageInError),

    #[error("Recieved extended socket error: {0}")]
    ExtendedSocketError(#[from] CSockExtendedError),

    #[error("Error while parsing socket control message: {0}")]
    ParseControlMessageTypeError(#[from] ParseControlMessageTypeError),
}

#[derive(Debug, Error)]
pub enum ParseControlMessageIpV6Error {

    #[error("Failed to retrieve packet info: {0}")]
    FailedToRetrievePacketInfo(#[source] FromMessageInError),

    #[error("Failed to retrieve hop limit: {0}")]
    FailedToRetrieveHopLimit(#[source] FromMessageInError),

    #[error("Failed to retrieve traffic class: {0}")]
    FailedToRetrieveTrafficClass(#[source] FromMessageInError),

    #[error("Failed to retrieve extended error: {0}")]
    FailedToRetrieveExtendedError(#[source] FromMessageInError),

    #[error("Recieved extended socket error: {0}")]
    ExtendedSocketError(#[from] CSockExtendedError),

    #[error("Error while parsing socket control message: {0}")]
    ParseControlMessageTypeError(#[from] ParseControlMessageTypeError),
}

#[derive(Debug, Error)]
pub enum ParseControlMessageTlsError {

    #[error("Failed to retrieve TLS Record Type: {0}")]
    FailedToRetrieveRecordType(#[source] FromMessageInError),

    #[error("Error while parsing socket control message: {0}")]
    ParseControlMessageTypeError(#[from] ParseControlMessageTypeError),
}

#[derive(Debug, Error)]
pub enum ParseControlMessageError {

    #[error("Error while parsing socket control message: {0}")]
    PrepareControlMessageTypeError(#[from] ParseControlMessageSocketError),

    #[error("Error while parsing IP V4 control message: {0}")]
    PrepareControlMessageIpV4Error(#[from] ParseControlMessageIpV4Error),

    #[error("Error while parsing IP V6 control message: {0}")]
    PrepareControlMessageIpV6Error(#[from] ParseControlMessageIpV6Error),

    #[error("Error while parsing TLS control message: {0}")]
    PrepareControlMessageTlsError(#[from] ParseControlMessageTlsError),

    #[error("Invalid control message level: {0}")]
    InvalidControlMessageLevel(libc::c_int),

    #[error("Null pointer")]
    NullPointer,

    #[error("Expected control message with length {expected}, but got {actual}")]
    InvalidControlMessageLength {
        expected: usize,
        actual: usize,
    },
}

#[derive(Debug, Error)]
pub enum PrepareControlMessageTypeError {
    #[error("Unsendable control message type {0}")]
    UnsendableControlMessageType(libc::c_int),
}

#[derive(Debug, Error)]
pub enum PrepareControlMessageSocketError {
    #[error("Failed to prepare socket control message: {0}")]
    PrepareControlMessageTypeError(#[from] PrepareControlMessageTypeError),
}

#[derive(Debug, Error)]
pub enum PrepareControlMessageIpV4Error {

    #[error("Failed to prepare packet info: {0}")]
    PacketInfoConversionError(#[from] PacketInfoConversionError),

    #[error("Expected IP V4 packet info, but got IP V6")]
    ExpectedIpV4PacketInfo,

    #[error("Failed to prepare socket control message: {0}")]
    PrepareControlMessageTypeError(#[from] PrepareControlMessageTypeError),
}

#[derive(Debug, Error)]
pub enum PrepareControlMessageIpV6Error {

    #[error("Failed to prepare packet info: {0}")]
    PacketInfoConversionError(#[from] PacketInfoConversionError),

    #[error("Expected IP V6 packet info, but got IP V4")]
    ExpectedIpV6PacketInfo,

    #[error("Failed to prepare socket control message: {0}")]
    PrepareControlMessageTypeError(#[from] PrepareControlMessageTypeError),
}

#[derive(Debug, Error)]
pub enum PrepareControlMessageTlsError {
    #[error("Failed to prepare socket control message: {0}")]
    PrepareControlMessageTypeError(#[from] PrepareControlMessageTypeError),
}

#[derive(Debug, Error)]
pub enum PrepareControlMessageError {
    #[error("Failed to prepare socket control message: {0}")]
    PrepareControlMessageTypeError(#[from] PrepareControlMessageSocketError),

    #[error("Failed to prepare IP V4 control message: {0}")]
    PrepareControlMessageIpV4Error(#[from] PrepareControlMessageIpV4Error),

    #[error("Failed to prepare IP V6 control message: {0}")]
    PrepareControlMessageIpV6Error(#[from] PrepareControlMessageIpV6Error),

    #[error("Failed to prepare TLS control message: {0}")]
    PrepareControlMessageTlsError(#[from] PrepareControlMessageTlsError),
}

impl PrepareControlMessage for IoControlMessageLevel {
    type Error = PrepareControlMessageError;

    fn prepare_control_message(self) -> Result<RawControlMessageOut, Self::Error> {
        match self {
            IoControlMessageLevel::Socket(level) => Ok(level.prepare_control_message()?),
            IoControlMessageLevel::IpV4(level) => Ok(level.prepare_control_message()?),
            IoControlMessageLevel::IpV6(level) => Ok(level.prepare_control_message()?),
            IoControlMessageLevel::Tls(level) => Ok(level.prepare_control_message()?),
        }
    }
}

impl ParseControlMessage for IoControlMessageLevel {
    type Error = ParseControlMessageError;

    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, Self::Error> {
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
                return Err(ParseControlMessageError::InvalidControlMessageLevel(
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
    type Error = ParseControlMessageSocketError;
    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, Self::Error> {

        match cmsg.message_type {
            libc::SCM_RIGHTS => {
                let fds_slice = unsafe {
                    from_msg_in_multi::<RawFd>(
                        &cmsg,
                        (cmsg.data_len / std::mem::size_of::<RawFd>()) as usize,
                    ).map_err(|e| {
                        ParseControlMessageSocketError::FailedToRetrieveFileDescriptors(e)
                    })?
                };
                let fds = fds_slice.to_vec();

                Ok(SocketLevelType::Rights(fds))
            },
            libc::SCM_CREDENTIALS => {
                let creds = unsafe {
                    from_msg_in_single::<libc::ucred>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageSocketError::FailedToRetrieveCredentials(e)
                        })
                    ?
                };
                Ok(SocketLevelType::Credentials(creds.into()))
            },
            libc::SCM_TIMESTAMP => {
            
                let tv = unsafe {
                    from_msg_in_single::<libc::timeval>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageSocketError::FailedToRetrieveTimestamp(e)
                        })
                    ?
                };

                let timestamp = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::new(
                    tv.tv_sec as u64,
                    tv.tv_usec as u32 * 1000,
                );

                Ok(SocketLevelType::Timestamp(timestamp))
              
            },
            libc::SCM_TIMESTAMPNS => {

                let ts = unsafe { 
                    from_msg_in_single::<libc::timespec>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageSocketError::FailedToRetrieveTimestamp(e)
                        })
                    ?
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
                    ).map_err(|e| {
                        ParseControlMessageSocketError::FailedToRetrieveTimestamp(e)
                    })?
                };

                if ts.len() != 3 {
                    return Err(ParseControlMessageSocketError::InvalidNumberOfTimestamps {
                        expected: 3,
                        actual: ts.len(),
                    });
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
                return Err(ParseControlMessageTypeError::InvalidControlMessageTypeForLevel {
                    message_type: cmsg.message_type,
                    message_level: libc::SOL_SOCKET,
                }.into());
            }
        }
    }
}

impl PrepareControlMessage for SocketLevelType {
    type Error = PrepareControlMessageSocketError;
    fn prepare_control_message(self) -> Result<RawControlMessageOut, Self::Error> {
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
                return Err(PrepareControlMessageTypeError::UnsendableControlMessageType(
                    libc::SCM_TIMESTAMPING,
                ).into());
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
    type Error = ParseControlMessageIpV4Error;
    
    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, Self::Error> {

        match cmsg.message_type {
            libc::IP_PKTINFO => {
                let pktinfo = unsafe { 
                    from_msg_in_single::<libc::in_pktinfo>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageIpV4Error::FailedToRetrievePacketInfo(e)
                        })
                    ?
                };
                Ok(IpV4LevelType::PacketInfo(pktinfo.into()))
            },
            libc::IP_TTL => {
                let ttl = unsafe {
                    from_msg_in_single::<i32>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageIpV4Error::FailedToRetrieveTimeToLive(e)
                        })
                    ?
                };
                Ok(IpV4LevelType::TimeToLive(ttl))
            },
            libc::IP_RECVERR => {
                let err = unsafe {
                    from_msg_in_single::<libc::sock_extended_err>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageIpV4Error::FailedToRetrieveExtendedError(e)
                        })
                    ?
                };
                Err(ParseControlMessageIpV4Error::ExtendedSocketError(err.into()))
            },
            _ => {
                return Err(ParseControlMessageTypeError::InvalidControlMessageTypeForLevel {
                    message_type: cmsg.message_type,
                    message_level: libc::IPPROTO_IP,
                }.into());
            }
        }
    }
}

impl PrepareControlMessage for IpV4LevelType {
    type Error = PrepareControlMessageIpV4Error;

    fn prepare_control_message(self) -> Result<RawControlMessageOut, Self::Error> {
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
                        return Err(PrepareControlMessageIpV4Error::ExpectedIpV4PacketInfo);
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
    type Error = ParseControlMessageIpV6Error;

    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, Self::Error> {

        match cmsg.message_type {
            libc::IPV6_PKTINFO => {
                let pktinfo = unsafe {
                    from_msg_in_single::<libc::in6_pktinfo>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageIpV6Error::FailedToRetrievePacketInfo(e)
                        })
                    ?
                };
                Ok(IpV6LevelType::PacketInfo(pktinfo.into()))
            },
            libc::IPV6_HOPLIMIT => {
                let hops = unsafe {
                    from_msg_in_single::<i32>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageIpV6Error::FailedToRetrieveHopLimit(e)
                        })
                    ?
                };
                Ok(IpV6LevelType::HopLimit(hops))
            },
            libc::IPV6_TCLASS => {
                let class = unsafe {
                    from_msg_in_single::<i32>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageIpV6Error::FailedToRetrieveTrafficClass(e)
                        })
                    ?
                };
                Ok(IpV6LevelType::Class((class as u8).into()))
            },
            libc::IPV6_RECVERR => {
                let err = unsafe {
                    from_msg_in_single::<libc::sock_extended_err>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageIpV6Error::FailedToRetrieveExtendedError(e)
                        })
                    ?
                };
                Err(ParseControlMessageIpV6Error::ExtendedSocketError(err.into()))
            },
            _ => {
                return Err(ParseControlMessageTypeError::InvalidControlMessageTypeForLevel {
                    message_type: cmsg.message_type,
                    message_level: libc::IPPROTO_IPV6,
                }.into());
            }
        }
    }
}

impl PrepareControlMessage for IpV6LevelType {

    type Error = PrepareControlMessageIpV6Error;

    fn prepare_control_message(self) -> Result<RawControlMessageOut, Self::Error> {
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
                        return Err(PrepareControlMessageIpV6Error::ExpectedIpV6PacketInfo);
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

    type Error = ParseControlMessageTlsError;

    fn parse_control_message(cmsg: RawControlMessageIn) -> Result<Self, Self::Error> {

        match cmsg.message_type {
            libc::TLS_GET_RECORD_TYPE => {
                let record_type = unsafe {
                    from_msg_in_single::<TlsRecordType>(&cmsg)
                        .map_err(|e| {
                            ParseControlMessageTlsError::FailedToRetrieveRecordType(e)
                        })
                    ?
                };
                Ok(TlsLevelType::GetRecordType(record_type))
            },
            _ => {
                return Err(ParseControlMessageTypeError::InvalidControlMessageTypeForLevel {
                    message_type: cmsg.message_type,
                    message_level: libc::SOL_TLS,
                }.into());
            }
        }
    }
}

impl PrepareControlMessage for TlsLevelType {

    type Error = PrepareControlMessageTlsError;

    fn prepare_control_message(self) -> Result<RawControlMessageOut, Self::Error> {
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
                return Err(PrepareControlMessageTypeError::UnsendableControlMessageType(
                    libc::TLS_GET_RECORD_TYPE,
                ).into());
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

    pub fn try_parse_single(cmsg_ptr: *const libc::cmsghdr) -> Result<Self, ParseControlMessageError> {

        if cmsg_ptr.is_null() {
            return Err(ParseControlMessageError::NullPointer);
        }

        let cmsg_val = unsafe { *cmsg_ptr };

        let base_size = unsafe { libc::CMSG_LEN(0) as usize };
        let hdr_size = cmsg_val.cmsg_len as usize;

        if hdr_size < base_size {
            return Err(ParseControlMessageError::InvalidControlMessageLength {
                expected: base_size,
                actual: hdr_size,
            });
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

    pub unsafe fn try_parse_from_slice(cmsgs: &[u8]) -> Result<Vec<Self>, ParseControlMessageError> {

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

    pub fn try_parse_from_msghdr(msghdr: &libc::msghdr) -> Result<Vec<Self>, ParseControlMessageError> {

        let cmsg_ptr = unsafe { libc::CMSG_FIRSTHDR(msghdr) };
        if cmsg_ptr.is_null() { return Err(ParseControlMessageError::NullPointer); }

        let mut messages = Vec::new();
        
        let mut current_cmsg_ptr = cmsg_ptr;
        while !current_cmsg_ptr.is_null() {
            let message = IoControlMessage::try_parse_single(current_cmsg_ptr)?;
            messages.push(message);

            current_cmsg_ptr = unsafe { libc::CMSG_NXTHDR(msghdr, current_cmsg_ptr) };
        }

        Ok(messages)
    }

    pub fn try_prepare(self) -> Result<Bytes, PrepareControlMessageError> {
        let raw_control_message = self.level.prepare_control_message()?;
        let space = unsafe { libc::CMSG_SPACE(raw_control_message.payload.len() as _) } as usize;
        let mut bytes = BytesMut::with_capacity(space);
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

        unsafe { bytes.advance_mut(space) };

        Ok(bytes.freeze())
    }

    pub fn try_prepare_multi(msgs: Vec<IoControlMessage>) -> Result<Bytes, PrepareControlMessageError> {

        use PrepareControlMessage;

        let mut prepared_messages = Vec::with_capacity(msgs.len());
        let mut total_size = 0;
        for msg in msgs {
            let raw_control_message = msg.level.prepare_control_message()?;
            let space = unsafe { libc::CMSG_SPACE(raw_control_message.payload.len() as _) } as usize;
            total_size += space;
            prepared_messages.push((raw_control_message, space));
        }

        let mut bytes = BytesMut::with_capacity(total_size);
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

        unsafe { bytes.advance_mut(offset) };

        Ok(bytes.freeze())
    }
}

#[derive(Debug)]
pub enum IoSendMessageDataBufferType {
    Bytes(Bytes),
    Vec(Arc<Vec<Bytes>>),
}

impl GenerateIoVecs for IoSendMessageDataBufferType {
    unsafe fn generate_iovecs(&self) -> Result<Vec<libc::iovec>, InvalidIoVecError> {
        match self {
            IoSendMessageDataBufferType::Bytes(buffer) => {
                unsafe { Ok(buffer.generate_iovecs()?) }
            },
            IoSendMessageDataBufferType::Vec(buffers) => {
                unsafe { Ok(buffers.generate_iovecs()?) }
            }
        }
    }
}

impl From<IoVecInputSource> for IoSendMessageDataBufferType {
    fn from(source: IoVecInputSource) -> Self {
        match source {
            IoVecInputSource::Single(buffer) => {
                IoSendMessageDataBufferType::Bytes(buffer)
            },
            IoVecInputSource::Multiple(buffers) => {
                IoSendMessageDataBufferType::Vec(buffers)
            }
        }
    }
}
                

#[derive(Debug)]
pub struct IoSendMessage {
    pub data: Option<IoSendMessageDataBufferType>,
    pub control: Option<Vec<IoControlMessage>>,
    pub address: Option<PeerAddress>,

    _private: (),
}

impl IoSendMessage {

    pub fn new(
        data: Option<IoSendMessageDataBufferType>,
        control: Option<Vec<IoControlMessage>>,
    ) -> Self {
        Self {
            data,
            control,
            address: None,
            _private: (),
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
        .set_data_buffer(Bytes::from(vec![
            TlsAlertLevel::Warning as u8,
            TlsAlertDescription::CloseNotify as u8,
        ]))
    }

    // Send simple buffer with no control
    pub fn send_buffer(buffer: Bytes) -> Self {
        Self {
            data: Some(IoSendMessageDataBufferType::Bytes(buffer)),
            control: None,
            address: None,
            _private: (),
        }
    }

    // Send multiple buffers with no control
    pub fn send_buffers(buffers: Arc<Vec<Bytes>>) -> Self {
        Self {
            data: Some(IoSendMessageDataBufferType::Vec(buffers)),
            control: None,
            address: None,
            _private: (),
        }
    }

    // Send simple buffer to specific address with no control
    pub fn send_buffer_to(
        buffer: Bytes,
        address: PeerAddress,
    ) -> Self {
        Self {
            data: Some(IoSendMessageDataBufferType::Bytes(buffer)),
            control: None,
            address: Some(address),
            _private: (),
        }
    }

    // Send multiple buffers to specific address with no control
    pub fn send_buffers_to(
        buffers: Arc<Vec<Bytes>>,
        address: PeerAddress,
    ) -> Self {
        Self {
            data: Some(IoSendMessageDataBufferType::Vec(buffers)),
            control: None,
            address: Some(address),
            _private: (),
        }
    }

    // Send fds on on Unix domain sockets
    pub fn send_fds(
        fds: Vec<RawFd>,
    ) -> Self {
        Self::default()
        .add_control_message(IoControlMessage::new(
            IoControlMessageLevel::Socket(SocketLevelType::Rights(fds)),
        ))
    }

    // Send credentials for authentication
    pub fn send_credentials(
        creds: Credentials,
    ) -> Self {
        Self::default()
        .add_control_message(IoControlMessage::new(
            IoControlMessageLevel::Socket(SocketLevelType::Credentials(creds)),
        ))
    }

    pub fn set_address(mut self, address: PeerAddress) -> Self {
        self.address = Some(address);
        self
    }

    pub fn clear_address(mut self) -> Self {
        self.address = None;
        self
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

        if let Some(existing_control) = &mut self.control {
            existing_control.extend(messages);
        }

        self
    }

    pub fn clear_control_messages(mut self) -> Self {
        self.control = None;
        self
    }

    pub fn set_data_buffer(
        mut self,
        buffer: Bytes
    ) -> Self {
        self.data = Some(IoSendMessageDataBufferType::Bytes(buffer));
        self
    }

    pub fn set_data_buffers(
        mut self,
        buffers: Arc<Vec<Bytes>>
    ) -> Self {
        self.data = Some(IoSendMessageDataBufferType::Vec(buffers));
        self
    }

    pub fn clear_data_buffers(mut self) -> Self {
        self.data = None;
        self
    }

    pub fn clear(mut self) -> Self {
        self.data = None;
        self.control = None;
        self.address = None;
        self
    }
}

impl Default for IoSendMessage {
    fn default() -> Self {
        Self {
            data: None,
            control: None,
            address: None,
            _private: (),
        }
    }
}

#[derive(Debug)]
pub struct IoRecvMessage {
    buffers: RecvMsgBuffers,
    address: Option<PeerAddress>,
    flags: IoRecvMsgOutputFlags,
    bytes_received: usize,
    control_bytes_received: usize,

    _parsed_control: Option<Vec<IoControlMessage>>,
}

impl IoRecvMessage {

    pub (crate) fn new(
        buffers: RecvMsgBuffers,
        _parsed_control: Option<Vec<IoControlMessage>>,
        address: Option<PeerAddress>,
        flags: IoRecvMsgOutputFlags,
        bytes_received: usize,
        control_bytes_received: usize,
    ) -> Self {
        Self {
            buffers,
            address,
            flags,
            _parsed_control,
            bytes_received,
            control_bytes_received,
        }
    }

    pub fn buffers(&self) -> &RecvMsgBuffers {
        &self.buffers
    }

    pub fn take_buffers(&mut self) -> RecvMsgBuffers {
        std::mem::take(&mut self.buffers)
    }

    pub fn control_messages(&self) -> Option<&Vec<IoControlMessage>> {
        self._parsed_control.as_ref()
    }

    pub fn address(&self) -> Option<&PeerAddress> {
        self.address.as_ref()
    }

    pub fn flags(&self) -> IoRecvMsgOutputFlags {
        self.flags
    }

    pub fn bytes_received(&self) -> usize {
        self.bytes_received
    }

    pub fn control_bytes_received(&self) -> usize {
        self.control_bytes_received
    }

    pub fn get_alert(&self) -> Option<TlsAlert> {
        if let Some(control) = &self._parsed_control {
            for msg in control {
                if let IoControlMessageLevel::Tls(TlsLevelType::GetRecordType(record_type)) = msg.level {
                    if record_type == TlsRecordType::Alert {
                        if let Some(buffers) = self.buffers.data() {
                            if buffers.len() == 1 && buffers[0].len() > 0 && buffers[0].len() <= 2 {
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

    pub fn is_fin(&self) -> bool {
        self.buffers.data().is_some() && self.bytes_received == 0
    }

}

pub enum IoMessage {
    Send(IoSendMessage),
    Recv(IoRecvMessage),
}

impl IoMessage {
    pub fn control(&self) -> Option<&Vec<IoControlMessage>> {
        match self {
            IoMessage::Send(msg) => msg.control.as_ref(),
            IoMessage::Recv(msg) => msg._parsed_control.as_ref(),
        }
    }

    pub fn address(&self) -> Option<&PeerAddress> {
        match self {
            IoMessage::Send(msg) => msg.address.as_ref(),
            IoMessage::Recv(msg) => msg.address.as_ref(),
        }
    }

    pub fn data_buffers(&self) -> Option<Vec<&[u8]>> {
        match self {
            IoMessage::Send(msg) => {
                msg.data.as_ref().map(|buffers| {
                    match buffers {
                        IoSendMessageDataBufferType::Bytes(buffer) => vec![buffer.as_ref()],
                        IoSendMessageDataBufferType::Vec(buffers) => buffers.iter().map(|buffer| buffer.as_ref()).collect(),
                    }
                })
            },
            IoMessage::Recv(msg) => {
                msg.buffers().data().map(|buffers| {
                    buffers.iter().map(|buffer| buffer.as_ref()).collect()
                })
            }
        }
    }

    pub fn n_data_buffers(&self) -> usize {
        match self {
            IoMessage::Send(msg) => {
                msg.data.as_ref().map_or(0, |buffers| {
                    match buffers {
                        IoSendMessageDataBufferType::Bytes(_) => 1,
                        IoSendMessageDataBufferType::Vec(buffers) => buffers.len(),
                    }
                })
            },
            IoMessage::Recv(msg) => msg.buffers().data().map_or(0, |buffers| buffers.len()),
        }
    }

    pub fn data_buffer_at(&self, index: usize) -> Option<&[u8]> {
        match self {
            IoMessage::Send(msg) => {
                msg.data.as_ref().and_then(|buffers| {
                    match buffers {
                        IoSendMessageDataBufferType::Bytes(buffer) => {
                            if index == 0 { Some(buffer.as_ref()) } else { None }
                        },
                        IoSendMessageDataBufferType::Vec(buffers) => buffers.get(index).map(|buffer| buffer.as_ref()),
                    }
                })
            },
            IoMessage::Recv(msg) => msg.buffers().data().and_then(|buffers| buffers.get(index)).map(|buffer| buffer.as_ref()),
        }
    }
}

pub struct PendingIoMessageInner {
    buffers: IoRecvMsgOutputBuffers,

    // Pointer structures
    _iovec: Option<Vec<libc::iovec>>,
    _msghdr: libc::msghdr,
    _addr: libc::sockaddr_storage,
}

impl std::fmt::Debug for PendingIoMessageInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingIoMessageInner")
            .field("buffers", &self.buffers)
            .finish()
    }
}

impl PendingIoMessageInner {

    pub fn new(
        mut buffers: IoRecvMsgOutputBuffers
    ) -> Result<Pin<Box<Self>>, IoSubmissionError> {

        let iovec_ret: Option<Result<Vec<libc::iovec>, InvalidIoVecError>> = buffers.data.as_mut().map(|buffers| {
            unsafe { buffers.generate_iovecs() }
        });

        let _iovec = match iovec_ret {
            Some(Ok(iovec)) => Some(iovec),
            Some(Err(e)) => {
                return Err(IoSubmissionError::from(
                    RecvMsgSubmissionError {
                        buffers: buffers.unwrap(),
                        kind: RecvMsgSubmissionErrorKind::InvalidIoVec(e)
                    }
                ));
            },
            None => None,
        };

        let addr_storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };

        let data = Self {
            buffers,
            _addr: addr_storage,
            _iovec,
            _msghdr: unsafe { std::mem::zeroed() },
        };

        let mut pinned_self = Box::pin(data);

        unsafe {
            let data_mut = Pin::get_unchecked_mut(pinned_self.as_mut());
            let msghdr = &mut data_mut._msghdr;

            msghdr.msg_name = &mut data_mut._addr as *mut libc::sockaddr_storage as *mut _;
            msghdr.msg_namelen = std::mem::size_of::<libc::sockaddr_storage>() as u32;

            msghdr.msg_iov = data_mut._iovec.as_mut().map_or(std::ptr::null_mut(), |iov| iov.as_mut_ptr() as *mut _);
            msghdr.msg_iovlen = data_mut._iovec.as_ref().map_or(0, |iov| iov.len() as _);

            msghdr.msg_control = data_mut.buffers.control.as_mut().map_or(std::ptr::null_mut(), |control| control.as_mut_ptr() as *mut _);
            msghdr.msg_controllen = data_mut.buffers.control.as_mut().as_mut().map_or(0, |control| control.writable_len() as _);
        }

        Ok(pinned_self)
    }
}

pub trait PendingIoMessageState {}

// In this state, we're waiting for the operation to be completed
pub struct PendingIoMessageStateWaiting;
impl PendingIoMessageState for PendingIoMessageStateWaiting {}

// In this state, We can get parsable data that doesn't mut the inner state
pub struct PendingIoMessageStateParsable { _bytes_received: usize }
impl PendingIoMessageState for PendingIoMessageStateParsable {}

// In this state, we can extract the inner data
pub struct PendingIoMessageStateParsed { bytes_received: usize }
impl PendingIoMessageState for PendingIoMessageStateParsed {}

#[derive(Debug)]
pub struct PendingIoMessage<T: PendingIoMessageState = PendingIoMessageStateWaiting> {
    // Using pin because PendingIoMessageInner is self-referential
    inner: Pin<Box<PendingIoMessageInner>>,
    state: T
}

impl PendingIoMessage<PendingIoMessageStateWaiting> {

    pub fn new(
        buffers: IoRecvMsgOutputBuffers,
    ) -> Result<Self, IoSubmissionError> {

        let inner = PendingIoMessageInner::new(
            buffers
        )?;

        Ok(Self {
            inner,
            state: PendingIoMessageStateWaiting,
        })
    }

    pub fn as_mut_ptr(&mut self) -> *mut libc::msghdr {
        let inner = unsafe {
            Pin::get_unchecked_mut(self.inner.as_mut())
        };

        &mut inner._msghdr
    }

    pub fn complete(
        self,
        _bytes_received: usize,
    ) -> PendingIoMessage<PendingIoMessageStateParsable> {
        PendingIoMessage {
            inner: self.inner,
            state: PendingIoMessageStateParsable { _bytes_received },
        }
    }
}

impl PendingIoMessage<PendingIoMessageStateParsable> {

    pub fn bytes_received(&self) -> usize { self.state._bytes_received }

    pub fn control_bytes_received(&self) -> usize {
        self.inner._msghdr.msg_controllen as usize
    }

    pub fn parse_address(&self) -> Result<Option<PeerAddress>, UnsupportedAddressFamilyError> {
        let addr_len = self.inner._msghdr.msg_namelen as usize;
        if addr_len == 0 { return Ok(None); }

        Ok(Some(PeerAddress::try_from(self.inner._addr)?))
    }

    pub fn parse_control(&self) -> Result<Option<Vec<IoControlMessage>>, ParseControlMessageError> {
        let control_len = self.inner._msghdr.msg_controllen as usize;
        if control_len == 0 { return Ok(None); }

        let control = IoControlMessage::try_parse_from_msghdr(
            &self.inner._msghdr,
        )?;

        Ok(Some(control))
    }

    pub fn parse_flags(&self) -> IoRecvMsgOutputFlags {
        let flags = self.inner._msghdr.msg_flags;
        IoRecvMsgOutputFlags::from_bits_truncate(flags)
    }
            
    pub fn next(self) -> PendingIoMessage<PendingIoMessageStateParsed> {
        PendingIoMessage {
            inner: self.inner,
            state: PendingIoMessageStateParsed { bytes_received: self.state._bytes_received },
        }
    }
}

impl PendingIoMessage<PendingIoMessageStateParsed> {

    pub fn bytes_received(&self) -> usize { self.state.bytes_received }

    pub fn control_bytes_received(&self) -> usize {
        self.inner._msghdr.msg_controllen as usize
    }

    pub fn extract_control_buffer(
        &mut self,
    ) -> Result<Option<BytesMut>, IoOutputBufferIntoBytesError> {

        let inner = unsafe {
            Pin::get_unchecked_mut(self.inner.as_mut())
        };

        Ok(match inner.buffers.control.take() {
            Some(control) => Some(
                control.into_bytes(inner._msghdr.msg_controllen as usize)?
            ),
            None => None,
        })
    }

    pub fn extract_data_buffers(
        &mut self,
    ) -> Result<Option<Vec<BytesMut>>, IoVecOutputBufferIntoBytesError> {

        let inner = unsafe {
            Pin::get_unchecked_mut(self.inner.as_mut())
        };

        Ok(match inner.buffers.data.take() {
            Some(buffers) => Some(
                buffers.into_bytes(self.state.bytes_received)?,
            ),
            None => None,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.inner.buffers.data.is_none() && self.inner.buffers.control.is_none()
    }
}

impl<T: PendingIoMessageState> PendingIoMessage<T> {

    /*
        SAFETY:
            This function is unsafe because it breaks the validity of the raw pointers.
            The caller must ensure the operation is not in progress when calling this function.
    */
    pub unsafe fn split(mut self) -> (
        Option<IoVecOutputBuffer>,
        Option<IoOutputBuffer>
    ) {
        let buffers = self.inner.buffers.data.take();
        let control = self.inner.buffers.control.take();

        (buffers, control)
    }

    pub unsafe fn into_buffers(
        mut self,
    ) -> IoRecvMsgOutputBuffers {
        IoRecvMsgOutputBuffers {
            data: self.inner.buffers.data.take(),
            control: self.inner.buffers.control.take(),
        }
    }

    pub fn buffers(&self) -> &IoRecvMsgOutputBuffers {
        &self.inner.buffers
    }
}

unsafe impl Send for PendingIoMessage<PendingIoMessageStateWaiting> {}
unsafe impl Sync for PendingIoMessage<PendingIoMessageStateWaiting> {}

pub struct PreparedIoMessageBuilder {
    buffers: Option<IoSendMessageDataBufferType>,
    control: Option<IoInputBuffer>,
    _control_len: usize,
    _iovec: Option<Vec<libc::iovec>>,
    _name: Option<libc::sockaddr_storage>,
    _name_len: u32,
}

impl PreparedIoMessageBuilder {

    pub fn new() -> Self {
        Self {
            buffers: None,
            _iovec: None,
            control: None,
            _name: None,
            _name_len: 0,
            _control_len: 0,
        }
    }

    pub fn set_buffers(
        &mut self, 
        buffers: IoSendMessageDataBufferType
    ) -> Result<&mut Self, IoSubmissionError> {
        self.buffers = Some(buffers);
        let buffers = self.buffers.as_mut().unwrap();
        self._iovec = Some ( unsafe { buffers.generate_iovecs()? } );
        Ok(self)
    }

    pub fn set_control(&mut self, control: IoInputBuffer) -> &mut Self {
        self._control_len = control.readable_len();
        self.control = Some(control);
        self
    }

    pub fn set_address(
        &mut self,
        address: PeerAddress,
    ) -> Result<&mut Self, IoSubmissionError> {

        let len = match address.ip().version() {
            IpVersion::V4 => std::mem::size_of::<libc::sockaddr_in>() as u32,
            IpVersion::V6 => std::mem::size_of::<libc::sockaddr_in6>() as u32,
        };

        let addr: libc::sockaddr_storage = address.into();

        self._name = Some(addr);
        self._name_len = len;
        Ok(self)
    }

    pub fn build(
        self,
    ) -> PreparedIoMessage {
        PreparedIoMessage {
            inner: PreparedIoMessageInner::new(
                self.buffers,
                self._iovec,
                self.control,
                self._control_len,
                self._name,
                self._name_len,
            ),
        }
    }
}

pub struct PreparedIoMessageInner {
    _buffers: Option<IoSendMessageDataBufferType>, // Application data buffers
    control: Option<IoInputBuffer>, // Control buffer
    address: Option<libc::sockaddr_storage>, // Address storage

    // Pointer structures
    _iovec: Option<Vec<libc::iovec>>,
    _msghdr: libc::msghdr,
}

impl PreparedIoMessageInner {

    pub fn new(
        buffers: Option<IoSendMessageDataBufferType>,
        _iovec: Option<Vec<libc::iovec>>,
        control: Option<IoInputBuffer>,
        _control_len: usize,
        _name: Option<libc::sockaddr_storage>,
        _name_len: u32,
    ) -> Pin<Box<Self>> {

        let data = Self {
            _buffers: buffers,
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
            msghdr.msg_controllen = _control_len;
        }

        pinned_self
    }
}

pub struct PreparedIoMessage {
    // Using pin because PreparedIoMessageInner is self-referential
    inner: Pin<Box<PreparedIoMessageInner>>,
}

impl PreparedIoMessage {

    pub fn as_ptr(&self) -> *const libc::msghdr {
        let inner = Pin::get_ref(self.inner.as_ref());

        &inner._msghdr
    }
}

unsafe impl Send for PreparedIoMessage {}
unsafe impl Sync for PreparedIoMessage {}

#[cfg(test)]
mod tests {
    use super::*; // Imports items from the parent module (message.rs)
    use crate::net::IpAddress; // If needed for canned messages
    use std::os::fd::RawFd;
    use std::time::{SystemTime, UNIX_EPOCH, Duration};

    // Helper to create a RawControlMessageIn for testing parsing
    // This is a bit simplified as it doesn't involve actual msghdr
    fn create_raw_cmsg_in<'a>(
        level: libc::c_int,
        ty: libc::c_int,
        data: &'a [u8],
    ) -> RawControlMessageIn<'a> {
        RawControlMessageIn {
            message_level: level,
            message_type: ty,
            data_len: data.len(),
            data_ptr: data.as_ptr(),
            _lifetime: std::marker::PhantomData,
        }
    }

    #[test]
    fn test_socket_rights_round_trip() {
        let fds_to_send: Vec<RawFd> = vec![3, 4, 5];
        let original_level = SocketLevelType::Rights(fds_to_send.clone());
        let control_msg = IoControlMessage::new(IoControlMessageLevel::Socket(original_level));

        let prepared_bytes = control_msg.try_prepare().expect("Preparation failed");

        // Simulate receiving this by parsing it back
        // In a real scenario, this would come from a msghdr
        let cmsg_hdr_ptr = prepared_bytes.as_ptr() as *const libc::cmsghdr;
        let parsed_control_msg = IoControlMessage::try_parse_single(cmsg_hdr_ptr)
            .expect("Parsing single failed");

        match parsed_control_msg.level() {
            IoControlMessageLevel::Socket(SocketLevelType::Rights(received_fds)) => {
                assert_eq!(received_fds, &fds_to_send);
            }
            _ => panic!("Parsed message was not SocketLevelType::Rights"),
        }
    }

    #[test]
    fn test_socket_credentials_round_trip() {
        let creds_to_send = Credentials { process_id: 123, user_id: 456, group_id: 789 };
        let original_level = SocketLevelType::Credentials(creds_to_send);
        let control_msg = IoControlMessage::new(IoControlMessageLevel::Socket(original_level));

        let prepared_bytes = control_msg.try_prepare().expect("Preparation failed");
        let cmsg_hdr_ptr = prepared_bytes.as_ptr() as *const libc::cmsghdr;
        let parsed_control_msg = IoControlMessage::try_parse_single(cmsg_hdr_ptr)
            .expect("Parsing single failed");

        match parsed_control_msg.level() {
            IoControlMessageLevel::Socket(SocketLevelType::Credentials(received_creds)) => {
                assert_eq!(received_creds.process_id, creds_to_send.process_id);
                assert_eq!(received_creds.user_id, creds_to_send.user_id);
                assert_eq!(received_creds.group_id, creds_to_send.group_id);
            }
            _ => panic!("Parsed message was not SocketLevelType::Credentials"),
        }
    }

    #[test]
    fn test_socket_timestamp_round_trip() {
        // Using a fixed time to avoid precision issues with SystemTime::now()
        let duration_since_epoch = Duration::new(1678886400, 123456000); // Example time
        let time_to_send = UNIX_EPOCH + duration_since_epoch;

        let original_level = SocketLevelType::Timestamp(time_to_send);
        let control_msg = IoControlMessage::new(IoControlMessageLevel::Socket(original_level));

        let prepared_bytes = control_msg.try_prepare().expect("Preparation failed");
        let cmsg_hdr_ptr = prepared_bytes.as_ptr() as *const libc::cmsghdr;
        let parsed_control_msg = IoControlMessage::try_parse_single(cmsg_hdr_ptr)
            .expect("Parsing single failed");

        match parsed_control_msg.level() {
            IoControlMessageLevel::Socket(SocketLevelType::Timestamp(received_time)) => {
                // libc::timeval has microsecond precision.
                // Compare by converting both to a common representation (e.g., micros since epoch)
                let original_micros = duration_since_epoch.as_micros();
                let received_micros = received_time.duration_since(UNIX_EPOCH).unwrap().as_micros();
                assert_eq!(received_micros, original_micros);
            }
            _ => panic!("Parsed message was not SocketLevelType::Timestamp"),
        }
    }

    #[test]
    fn test_socket_detailed_timestamp_parsing() {
        // DetailedTimestamp is not sendable, so we only test parsing
        let mut timespecs_data = Vec::new();
        let ts1 = libc::timespec { tv_sec: 100, tv_nsec: 200 }; // Software
        let ts2 = libc::timespec { tv_sec: 0, tv_nsec: 0 };   // Legacy hardware (unused by SCM_TIMESTAMPING)
        let ts3 = libc::timespec { tv_sec: 101, tv_nsec: 300 }; // Hardware / Transformed

        // Manually construct the byte payload for [ts1, ts2, ts3]
        timespecs_data.extend_from_slice(unsafe {
            std::slice::from_raw_parts(&ts1 as *const _ as *const u8, std::mem::size_of::<libc::timespec>())
        });
        timespecs_data.extend_from_slice(unsafe {
            std::slice::from_raw_parts(&ts2 as *const _ as *const u8, std::mem::size_of::<libc::timespec>())
        });
         timespecs_data.extend_from_slice(unsafe {
            std::slice::from_raw_parts(&ts3 as *const _ as *const u8, std::mem::size_of::<libc::timespec>())
        });

        let raw_cmsg = create_raw_cmsg_in(libc::SOL_SOCKET, libc::SCM_TIMESTAMPING, &timespecs_data);
        let parsed_level = SocketLevelType::parse_control_message(raw_cmsg).expect("Parsing SCM_TIMESTAMPING failed");

        match parsed_level {
            SocketLevelType::DetailedTimestamp { software, hardware } => {
                assert_eq!(software.duration_since(UNIX_EPOCH).unwrap().as_secs(), ts1.tv_sec as u64);
                assert_eq!(software.duration_since(UNIX_EPOCH).unwrap().subsec_nanos(), ts1.tv_nsec as u32);
                assert_eq!(hardware.duration_since(UNIX_EPOCH).unwrap().as_secs(), ts3.tv_sec as u64);
                assert_eq!(hardware.duration_since(UNIX_EPOCH).unwrap().subsec_nanos(), ts3.tv_nsec as u32);
            }
            _ => panic!("Parsed message was not SocketLevelType::DetailedTimestamp"),
        }
    }

    #[test]
    fn test_socket_detailed_timestamp_prepare_fails() {
        let level = SocketLevelType::DetailedTimestamp {
            software: SystemTime::now(),
            hardware: SystemTime::now(),
        };
        let result = level.prepare_control_message();
        assert!(result.is_err());
        match result.unwrap_err() {
            PrepareControlMessageSocketError::PrepareControlMessageTypeError(
                PrepareControlMessageTypeError::UnsendableControlMessageType(ty)
            ) => {
                assert_eq!(ty, libc::SCM_TIMESTAMPING);
            }
        }
    }

    #[test]
    fn test_ipv4_packetinfo_round_trip() {
        let pktinfo_to_send = PacketInfo {
            interface_index: 2,
            destination: IpAddress::V4([192, 168, 1, 10]),
            specific_destination: Some(IpAddress::V4([192, 168, 1, 20])),
        };
        let original_level = IpV4LevelType::PacketInfo(pktinfo_to_send);
        let control_msg = IoControlMessage::new(IoControlMessageLevel::IpV4(original_level));

        let prepared_bytes = control_msg.try_prepare().expect("Preparation failed");
        let cmsg_hdr_ptr = prepared_bytes.as_ptr() as *const libc::cmsghdr;
        let parsed_control_msg = IoControlMessage::try_parse_single(cmsg_hdr_ptr)
            .expect("Parsing single failed");

        match parsed_control_msg.level() {
            IoControlMessageLevel::IpV4(IpV4LevelType::PacketInfo(received_pktinfo)) => {
                assert_eq!(received_pktinfo.interface_index, pktinfo_to_send.interface_index);
                assert_eq!(received_pktinfo.destination, pktinfo_to_send.destination);
                assert_eq!(received_pktinfo.specific_destination, pktinfo_to_send.specific_destination);
            }
            _ => panic!("Parsed message was not IpV4LevelType::PacketInfo"),
        }
    }

    #[test]
    fn test_ipv4_ttl_round_trip() {
        let ttl_to_send = 64;
        let original_level = IpV4LevelType::TimeToLive(ttl_to_send);
        let control_msg = IoControlMessage::new(IoControlMessageLevel::IpV4(original_level));

        let prepared_bytes = control_msg.try_prepare().expect("Preparation failed");
        let cmsg_hdr_ptr = prepared_bytes.as_ptr() as *const libc::cmsghdr;
        let parsed_control_msg = IoControlMessage::try_parse_single(cmsg_hdr_ptr)
            .expect("Parsing single failed");

        match parsed_control_msg.level() {
            IoControlMessageLevel::IpV4(IpV4LevelType::TimeToLive(received_ttl)) => {
                assert_eq!(*received_ttl, ttl_to_send);
            }
            _ => panic!("Parsed message was not IpV4LevelType::TimeToLive"),
        }
    }

    #[test]
    fn test_ipv6_packetinfo_round_trip() {
        let pktinfo_to_send = PacketInfo {
            interface_index: 3,
            destination: IpAddress::V6([0x20,0x01,0x0d,0xb8,0,0,0,0,0,0,0,0,0,0,0,1]),
            specific_destination: None, // IPv6 pktinfo doesn't have spec_dst in the same way
        };
        let original_level = IpV6LevelType::PacketInfo(pktinfo_to_send);
        let control_msg = IoControlMessage::new(IoControlMessageLevel::IpV6(original_level));

        let prepared_bytes = control_msg.try_prepare().expect("Preparation failed");
        let cmsg_hdr_ptr = prepared_bytes.as_ptr() as *const libc::cmsghdr;
        let parsed_control_msg = IoControlMessage::try_parse_single(cmsg_hdr_ptr)
            .expect("Parsing single failed");

        match parsed_control_msg.level() {
            IoControlMessageLevel::IpV6(IpV6LevelType::PacketInfo(received_pktinfo)) => {
                assert_eq!(received_pktinfo.interface_index, pktinfo_to_send.interface_index);
                assert_eq!(received_pktinfo.destination, pktinfo_to_send.destination);
            }
            _ => panic!("Parsed message was not IpV6LevelType::PacketInfo"),
        }
    }
    // Add tests for IpV6LevelType::HopLimit and IpV6LevelType::Class similarly

    #[test]
    fn test_tls_get_record_type_parsing() {
        // This tests parsing what the kernel might send
        let record_type_byte = TlsRecordType::Alert as u8;
        let data = [record_type_byte];
        let raw_cmsg = create_raw_cmsg_in(libc::SOL_TLS, libc::TLS_GET_RECORD_TYPE, &data);
        let parsed_level = TlsLevelType::parse_control_message(raw_cmsg)
            .expect("Parsing TLS_GET_RECORD_TYPE failed");

        match parsed_level {
            TlsLevelType::GetRecordType(received_rt) => {
                assert_eq!(received_rt, TlsRecordType::Alert);
            }
            _ => panic!("Parsed message was not TlsLevelType::GetRecordType"),
        }
    }

    #[test]
    fn test_tls_get_record_type_prepare_fails() {
        let level = TlsLevelType::GetRecordType(TlsRecordType::Alert); // GetRecordType is not sendable
        let result = level.prepare_control_message();
        assert!(result.is_err());
         match result.unwrap_err() {
            PrepareControlMessageTlsError::PrepareControlMessageTypeError(
                PrepareControlMessageTypeError::UnsendableControlMessageType(ty)
            ) => {
                assert_eq!(ty, libc::TLS_GET_RECORD_TYPE);
            }
        }
    }

    #[test]
    fn test_try_parse_multi() {
        let creds = Credentials { process_id: 1, user_id: 2, group_id: 3 };
        let msg1 = IoControlMessage::new(IoControlMessageLevel::Socket(SocketLevelType::Credentials(creds)));

        let ttl = 64;
        let msg2 = IoControlMessage::new(IoControlMessageLevel::IpV4(IpV4LevelType::TimeToLive(ttl)));

        let combined_bytes = IoControlMessage::try_prepare_multi(vec![msg1, msg2.clone()]) // Clone msg2 if needed later
            .expect("Multi preparation failed");

        // Simulate receiving this combined buffer
        let parsed_messages = unsafe { IoControlMessage::try_parse_from_slice(&combined_bytes) }
            .expect("Parsing multi from slice failed");

        assert_eq!(parsed_messages.len(), 2);

        match parsed_messages[0].level() {
            IoControlMessageLevel::Socket(SocketLevelType::Credentials(rcv_creds)) => {
                assert_eq!(rcv_creds.process_id, creds.process_id);
            }
            _ => panic!("First parsed message was not Credentials"),
        }
        match parsed_messages[1].level() {
            IoControlMessageLevel::IpV4(IpV4LevelType::TimeToLive(rcv_ttl)) => {
                assert_eq!(*rcv_ttl, ttl);
            }
            _ => panic!("Second parsed message was not TimeToLive"),
        }
    }

    #[test]
    fn test_parse_invalid_data_length() {
        let data = [1u8, 2u8]; // Too short for SCM_CREDENTIALS (libc::ucred)
        let raw_cmsg = create_raw_cmsg_in(libc::SOL_SOCKET, libc::SCM_CREDENTIALS, &data);
        let result = SocketLevelType::parse_control_message(raw_cmsg);
        assert!(result.is_err());
        match result.unwrap_err() {
            ParseControlMessageSocketError::FailedToRetrieveCredentials(FromMessageInError { expected, actual }) => {
                assert_eq!(actual, data.len());
                assert_eq!(expected, std::mem::size_of::<libc::ucred>());
            }
            _ => panic!("Unexpected error type for invalid data length"),
        }
    }

    #[test]
    fn test_parse_invalid_message_type_for_level() {
        let data = [0u8; 16]; // Dummy data
        // Using SCM_RIGHTS type with IPPROTO_IP level (which is wrong)
        let raw_cmsg = create_raw_cmsg_in(libc::IPPROTO_IP, libc::SCM_RIGHTS, &data);
        let result = IoControlMessageLevel::parse_control_message(raw_cmsg);
        assert!(result.is_err());
        match result.unwrap_err() {
            ParseControlMessageError::PrepareControlMessageIpV4Error(
                ParseControlMessageIpV4Error::ParseControlMessageTypeError(
                    ParseControlMessageTypeError::InvalidControlMessageTypeForLevel { message_type, message_level }
                )
            ) => {
                assert_eq!(message_type, libc::SCM_RIGHTS);
                assert_eq!(message_level, libc::IPPROTO_IP);
            }
            e => panic!("Unexpected error type for invalid message type for level: {:?}", e),
        }
    }

    #[test]
    fn test_parse_invalid_message_level() {
        let data = [0u8; 16]; // Dummy data
        let invalid_level = 9999; // An unused level
        let raw_cmsg = create_raw_cmsg_in(invalid_level, libc::SCM_RIGHTS, &data);
        let result = IoControlMessageLevel::parse_control_message(raw_cmsg);
        assert!(result.is_err());
        match result.unwrap_err() {
            ParseControlMessageError::InvalidControlMessageLevel(level) => {
                assert_eq!(level, invalid_level);
            }
            e => panic!("Unexpected error type for invalid message level: {:?}", e),
        }
    }
}