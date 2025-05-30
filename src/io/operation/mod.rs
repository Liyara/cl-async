use std::{fmt, os::fd::{
    AsRawFd, 
    RawFd
}, path::Path};

use bytes::BytesMut;
use enum_dispatch::enum_dispatch;
use crate::{net::PeerAddress, OsError};
use crate::Key;
use super::{buffers::{IoRecvMsgOutputBuffers, RecvMsgBuffersRefs}, IoBytesMutRecovery, IoBytesMutVecRecovery, IoCompletion, IoRecvMessageRecovery, OutputBufferVecSubmissionError, RecvMsgBuffers};
use super::IoFailure;
use data::{AsUringEntry, CompletableOperation};
use super::{buffers::{IoVecInputBuffer, IoVecOutputBuffer}, message::IoSendMessage, IoCompletionResult, IoInputBuffer, IoOutputBuffer, IoSubmissionError};

pub mod data;

#[macro_use]
pub mod future;

#[enum_dispatch(CompletableOperation, AsUringEntry)]
pub enum IoType {
    Read(data::IoReadData),
    Write(data::IoWriteData),
    Accept(data::IoAcceptData),
    Close(data::IoCloseData),
    Recv(data::IoRecvData),
    Send(data::IoSendData),
    Readv(data::IoReadvData),
    Writev(data::IoWritevData),
    RecvMsg(data::IoRecvMsgData),
    SendMsg(data::IoSendMsgData),
    Connect(data::IoConnectData),
    Shutdown(data::IoShutdownData),
    OpenAt(data::IoOpenAtData),
    Statx(data::IoStatxData),
    Unlink(data::IoUnlinkData),
    Rename(data::IoRenameData),
    MkDir(data::IoMkdirData),
    Cancel(data::IoCancelData),
    Splice(data::IoSpliceData),
    FSync(data::IoFSyncData),
    Fdatasync(data::IoFDatasyncData),
    Timeout(data::IoTimeoutData),
    AcceptMulti(data::IoAcceptMultiData),
}

impl fmt::Debug for IoType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoType::Read(_) => write!(f, "IoType::Read"),
            IoType::Write(_) => write!(f, "IoType::Write"),
            IoType::Accept(_) => write!(f, "IoType::Accept"),
            IoType::Close(_) => write!(f, "IoType::Close"),
            IoType::Recv(_) => write!(f, "IoType::Recv"),
            IoType::Send(_) => write!(f, "IoType::Send"),
            IoType::Readv(_) => write!(f, "IoType::Readv"),
            IoType::Writev(_) => write!(f, "IoType::Writev"),
            IoType::RecvMsg(_) => write!(f, "IoType::RecvMsg"),
            IoType::SendMsg(_) => write!(f, "IoType::SendMsg"),
            IoType::Connect(_) => write!(f, "IoType::Connect"),
            IoType::Shutdown(_) => write!(f, "IoType::Shutdown"),
            IoType::OpenAt(_) => write!(f, "IoType::OpenAt"),
            IoType::Statx(_) => write!(f, "IoType::Statx"),
            IoType::Unlink(_) => write!(f, "IoType::Unlink"),
            IoType::Rename(_) => write!(f, "IoType::Rename"),
            IoType::MkDir(_) => write!(f, "IoType::MkDir"),
            IoType::Cancel(_) => write!(f, "IoType::Cancel"),
            IoType::Splice(_) => write!(f, "IoType::Splice"),
            IoType::FSync(_) => write!(f, "IoType::FSync"),
            IoType::Timeout(_) => write!(f, "IoType:Timeout"),
            IoType::AcceptMulti(_) => write!(f, "IoOperation:AcceptMulti"),
            IoType::Fdatasync(_) => write!(f, "IoType::Fdatasync"),
        }
    }
}

impl IoBytesMutRecovery for IoType {
    fn as_bytes_mut(&self) -> Option<&BytesMut> {
        match self {
            IoType::Read(data) => data.as_bytes_mut(),
            IoType::Recv(data) => data.as_bytes_mut(),
            IoType::RecvMsg(data) => data.as_bytes_mut(),
            _ => None,
        }
    }

    fn into_bytes_mut(self) -> Option<BytesMut> {
        match self {
            IoType::Read(data) => data.into_bytes_mut(),
            IoType::Recv(data) => data.into_bytes_mut(),
            IoType::RecvMsg(data) => data.into_bytes_mut(),
            _ => None,
        }
    }
    
    fn take_bytes_mut(&mut self) -> Option<BytesMut> {
        match self {
            IoType::Read(data) => data.take_bytes_mut(),
            IoType::Recv(data) => data.take_bytes_mut(),
            IoType::RecvMsg(data) => data.take_bytes_mut(),
            _ => None,
        }
    }
}
    

impl IoBytesMutVecRecovery for IoType {
    fn as_vec(&self) -> Option<&Vec<BytesMut>> {
        match self {
            IoType::Readv(data) => data.as_vec(),
            IoType::RecvMsg(data) => data.as_vec(),
            _ => None,
        }
    }

    fn into_vec(self) -> Option<Vec<BytesMut>> {
        match self {
            IoType::Readv(data) => data.into_vec(),
            IoType::RecvMsg(data) => data.into_vec(),
            _ => None,
        }  
    }
    
    fn take_vec(&mut self) -> Option<Vec<BytesMut>> {
        match self {
            IoType::Readv(data) => data.take_vec(),
            IoType::RecvMsg(data) => data.take_vec(),
            _ => None,
        }
    }
}

impl IoRecvMessageRecovery for IoType {
    fn as_recvmsg_buffers(&self) -> Option<RecvMsgBuffersRefs<'_>> {
        match self {
            IoType::RecvMsg(data) => data.as_recvmsg_buffers(),
            _ => None,
        }
    }

    fn into_recvmsg_buffers(self) -> Option<RecvMsgBuffers> {
        match self {
            IoType::RecvMsg(data) => data.into_recvmsg_buffers(),
            _ => None,
        }
    }
    
    fn take_recvmsg_buffers(&mut self) -> Option<RecvMsgBuffers> {
        match self {
            IoType::RecvMsg(data) => data.take_recvmsg_buffers(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoDuration {
    Zero,
    Single,
    Persistent,
}

#[derive(Debug)]
pub struct IoOperation {
    fd: RawFd,
    t: IoType,
    duration: IoDuration,
}

impl IoOperation {

    pub fn io_type(&self) -> &IoType { &self.t }
    pub fn io_type_mut(&mut self) -> &mut IoType { &mut self.t }
    pub fn duration(&self) -> IoDuration { self.duration }
    pub fn fd(&self) -> RawFd { self.fd }
    pub fn into_type(self) -> IoType { self.t }


    /*
    
        READ operations

    */

    pub fn read<T: AsRawFd>(fd: &T, len: usize) -> Self {
        Self::read_into(fd, IoOutputBuffer::with_capacity(len))
    }

    pub fn read_at<T: AsRawFd>(fd: &T, offset: usize, len: usize) -> Self {
        Self::read_at_into(fd, offset, IoOutputBuffer::with_capacity(len))
    }

    pub fn read_into<T: AsRawFd>(fd: &T, buffer: IoOutputBuffer) -> Self {
        Self::read_at_into(fd, 0, buffer)
    }

    pub fn read_at_into<T: AsRawFd>(fd: &T, offset: usize, buffer: IoOutputBuffer) -> Self {
        Self {
            fd: fd.as_raw_fd(),
            t: IoType::Read(data::IoReadData::new(
                buffer,
                offset
            )),
            duration: IoDuration::Single,
        }
    }

    pub fn recv<T: AsRawFd>(fd: &T, len: usize, flags: data::IoRecvFlags) -> Self {
        Self::recv_into(fd, IoOutputBuffer::with_capacity(len), flags)
    }

    pub fn recv_into<T: AsRawFd>(fd: &T, buffer: IoOutputBuffer, flags: data::IoRecvFlags) -> Self {
        Self {
            fd: fd.as_raw_fd(),
            t: IoType::Recv(data::IoRecvData::new(
                buffer,
                flags
            )),
            duration: IoDuration::Single,
        }
    }

    pub fn readv<T: AsRawFd>(
        fd: &T, 
        buffers_lengths: Vec<usize>
    ) -> Result<Self, IoSubmissionError> {
        Self::readv_at(fd, 0, buffers_lengths)
    }

    pub fn readv_at<T: AsRawFd>(
        fd: &T, 
        offset: usize, 
        buffers_lengths: Vec<usize>
    ) -> Result<Self, IoSubmissionError> {
        let mut buffers = Vec::with_capacity(buffers_lengths.len());
        for len in buffers_lengths {
            buffers.push(BytesMut::with_capacity(len));
        }
        Self::readv_at_into(
            fd, 
            offset, 
            IoVecOutputBuffer::new(buffers)
                .map_err(OutputBufferVecSubmissionError::from)
            ?
        )
    }

    pub fn readv_into<T: AsRawFd>(
        fd: &T, 
        buffers: IoVecOutputBuffer,
    ) -> Result<Self, IoSubmissionError> {
        Self::readv_at_into(fd, 0, buffers)
    }

    pub fn readv_at_into<T: AsRawFd>(
        fd: &T, 
        offset: usize, 
        buffers: IoVecOutputBuffer
    ) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: fd.as_raw_fd(),
            t: IoType::Readv(data::IoReadvData::new(
                buffers,
                offset
            )?),
            duration: IoDuration::Single,
        })
    }

    pub fn recv_msg<T: AsRawFd>(
        fd: &T,
        buffer_lengths: Vec<usize>,
        control_length: usize,
        flags: data::IoRecvMsgInputFlags,
    ) -> Result<Self, IoSubmissionError> {

        let data = if buffer_lengths.len() > 0 {Some(
                IoVecOutputBuffer::new(
                buffer_lengths.into_iter()
                    .map(|len| BytesMut::with_capacity(len))
                    .collect::<Vec<_>>()
            ).map_err(OutputBufferVecSubmissionError::from)?
        )} else { None };

        let control = if control_length > 0 {
            Some(IoOutputBuffer::with_capacity(control_length))
        } else {
            None
        };

        Self::recv_msg_into(
            fd,
            IoRecvMsgOutputBuffers {
                data,
                control,
            },
            flags,
        )
    }

    pub fn recv_msg_into<T: AsRawFd>(
        fd: &T,
        buffers: IoRecvMsgOutputBuffers,
        flags: data::IoRecvMsgInputFlags,
    ) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: fd.as_raw_fd(),
            t: IoType::RecvMsg(data::IoRecvMsgData::new(
                buffers,
                flags,
            )?),
            duration: IoDuration::Single,
        })
    }

    /*

        WRITE operations

    */

    pub fn write<T: AsRawFd>(fd: &T, buffer: IoInputBuffer) -> Self {
        Self::write_at(fd, 0, buffer)
    }

    pub fn write_at<T: AsRawFd>(fd: &T, offset: usize, buffer: IoInputBuffer) -> Self {
        Self {
            fd: fd.as_raw_fd(),
            t: IoType::Write(data::IoWriteData::new(
                buffer,
                offset,
            )),
            duration: IoDuration::Single,
        }
    }

    pub fn send<T: AsRawFd>(fd: &T, buffer: IoInputBuffer, flags: data::IoSendFlags) -> Self {
        Self {
            fd: fd.as_raw_fd(),
            t: IoType::Send(data::IoSendData::new(
                buffer,
                flags,
            )),
            duration: IoDuration::Single,
        }
    }

    pub fn send_msg<T: AsRawFd>(
        fd: &T,
        message: IoSendMessage,
        flags: data::IoSendFlags,
    ) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: fd.as_raw_fd(),
            t: IoType::SendMsg(data::IoSendMsgData::new(
                message,
                flags,
            )?),
            duration: IoDuration::Single,
        })
    }

    pub fn writev<T: AsRawFd>(fd: &T, buffers: IoVecInputBuffer) -> Result<Self, IoSubmissionError> {
        Self::writev_at(fd, 0, buffers)
    }

    pub fn writev_at<T: AsRawFd>(fd: &T, offset: usize, buffers: IoVecInputBuffer) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: fd.as_raw_fd(),
            t: IoType::Writev(data::IoWritevData::new(
                buffers,
                offset
            )?),
            duration: IoDuration::Single,
        })
    }

    pub fn splice<T: AsRawFd, U: AsRawFd>(
        fd_in: &T,
        fd_out: &U,
        in_offset: usize,
        out_offset: usize,
        len: usize,
        flags: data::IoSpliceFlags
    ) -> Self {
        Self {
            fd: fd_in.as_raw_fd(),
            t: IoType::Splice(data::IoSpliceData {
                fd_out: fd_out.as_raw_fd(),
                in_offset,
                out_offset,
                bytes: len,
                flags,
            }),
            duration: IoDuration::Single,
        }
    }

    /*

        Socket Management operations 

    */

    pub fn accept<T: AsRawFd>(fd: &T) -> Self {
        Self {
            fd: fd.as_raw_fd(),
            t: IoType::Accept(data::IoAcceptData::new()),
            duration: IoDuration::Single,
        }
    }

    pub fn accept_multi<T: AsRawFd>(fd: &T) -> Self {
        Self {
            fd: fd.as_raw_fd(),
            t: IoType::AcceptMulti(data::IoAcceptMultiData {}),
            duration: IoDuration::Persistent,
        }
    }

    fn _connect<T: AsRawFd>(fd: &T, addr: PeerAddress, duration: IoDuration) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: fd.as_raw_fd(),
            t: IoType::Connect(data::IoConnectData::new(addr)?),
            duration,
        })
    }

    pub fn connect<T: AsRawFd>(fd: &T, addr: PeerAddress) -> Result<Self, IoSubmissionError> {
        Self::_connect(fd, addr, IoDuration::Single)
    }

    pub fn connect_forget<T: AsRawFd>(fd: &T, addr: PeerAddress) -> Result<Self, IoSubmissionError> {
        Self::_connect(fd, addr, IoDuration::Zero)
    }

    fn _shutodwn<T: AsRawFd>(fd: &T, how: data::IoShutdownType, duration: IoDuration) -> Self {
        Self {
            fd: fd.as_raw_fd(),
            t: IoType::Shutdown(data::IoShutdownData::new(how)),
            duration
        }
    }

    pub fn shutdown_read<T: AsRawFd>(fd: &T) -> Self {
        Self::_shutodwn(fd, data::IoShutdownType::Read, IoDuration::Single)
    }

    pub fn shutdown_read_forget<T: AsRawFd>(fd: &T) -> Self {
        Self::_shutodwn(fd, data::IoShutdownType::Read, IoDuration::Zero)
    }

    pub fn shutdown_write<T: AsRawFd>(fd: &T) -> Self {
        Self::_shutodwn(fd, data::IoShutdownType::Write, IoDuration::Single)
    }

    pub fn shutdown_write_forget<T: AsRawFd>(fd: &T) -> Self {
        Self::_shutodwn(fd, data::IoShutdownType::Write, IoDuration::Zero)
    }

    pub fn shutdown<T: AsRawFd>(fd: &T) -> Self {
        Self::_shutodwn(fd, data::IoShutdownType::Both, IoDuration::Single)
    }

    pub fn shutdown_forget<T: AsRawFd>(fd: &T) -> Self {
        Self::_shutodwn(fd, data::IoShutdownType::Both, IoDuration::Zero)
    }

    /*

        File System operations

    */

    pub fn close<T: AsRawFd>(fd: &T) -> Self {
        let fd = fd.as_raw_fd();
        Self {
            fd,
            t: IoType::Close(data::IoCloseData {}),
            duration: IoDuration::Single,
        }
    }

    pub fn close_forget<T: AsRawFd>(fd: &T) -> Self {
        let fd = fd.as_raw_fd();
        Self {
            fd,
            t: IoType::Close(data::IoCloseData {}),
            duration: IoDuration::Zero,
        }
    }

    pub fn open(
        path: &Path,
        settings: data::IoFileOpenSettings
    ) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: libc::AT_FDCWD,
            t: IoType::OpenAt(data::IoOpenAtData::new(
                path,
                settings
            )?),
            duration: IoDuration::Single,
        })
    }

    pub fn stats_fd<T: AsRawFd>(
        fd: &T,
        flags: data::IoStatxFlags,
        mask: data::IoStatxMask,
    ) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: fd.as_raw_fd(),
            t: IoType::Statx(data::IoStatxData::new(
                &Path::new(""),
                data::IoStatxFlags::AT_EMPTY_PATH | flags,
                mask,
            )?),
            duration: IoDuration::Single,
        })
    }

    pub fn stats_path(
        path: &Path,
        flags: data::IoStatxFlags,
        mask: data::IoStatxMask,
    ) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: libc::AT_FDCWD,
            t: IoType::Statx(data::IoStatxData::new(
                path,
                flags,
                mask
            )?),
            duration: IoDuration::Single,
        })
    }

    fn _unlink(
        path: &Path,
        duration: IoDuration,
    ) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: libc::AT_FDCWD,
            t: IoType::Unlink(data::IoUnlinkData::new(path)?),
            duration
        })
    }

    pub fn unlink(path: &Path) -> Result<Self, IoSubmissionError> {
        Self::_unlink(path, IoDuration::Single)
    }

    pub fn unlink_forget(path: &Path) -> Result<Self, IoSubmissionError> {
        Self::_unlink(path, IoDuration::Zero)
    }

    fn _mkdir(
        path: &Path,
        mode: data::IoFileSystemMode,
        duration: IoDuration,
    ) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: libc::AT_FDCWD,
            t: IoType::MkDir(data::IoMkdirData::new(
                path,
                mode
            )?),
            duration
        })
    }

    pub fn mkdir(path: &Path, mode: data::IoFileSystemMode) -> Result<Self, IoSubmissionError> {
        Self::_mkdir(path, mode, IoDuration::Single)
    }

    pub fn mkdir_forget(path: &Path, mode: data::IoFileSystemMode) -> Result<Self, IoSubmissionError> {
        Self::_mkdir(path, mode, IoDuration::Zero)
    }

    fn _rename(
        old_path: &Path,
        new_path: &Path,
        duration: IoDuration,
    ) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            fd: libc::AT_FDCWD,
            t: IoType::Rename(data::IoRenameData::new(
                old_path,
                new_path
            )?),
            duration
        })
    }

    pub fn rename(old_path: &Path, new_path: &Path) -> Result<Self, IoSubmissionError> {
        Self::_rename(old_path, new_path, IoDuration::Single)
    }

    pub fn rename_forget(old_path: &Path, new_path: &Path) -> Result<Self, IoSubmissionError> {
        Self::_rename(old_path, new_path, IoDuration::Zero)
    }

    /*

        Submission operations
    
    */

    fn _cancel(
        key: Key,
        duration: IoDuration,
    ) -> Self {
        Self {
            fd: libc::AT_FDCWD,
            t: IoType::Cancel(data::IoCancelData::new(key)),
            duration
        }
    }

    pub fn cancel(key: Key) -> Self {
        Self::_cancel(key, IoDuration::Single)
    }

    pub fn cancel_forget(key: Key) -> Self {
        Self::_cancel(key, IoDuration::Zero)
    }

    pub fn timeout(
        duration: std::time::Duration,
    ) -> Self {
        Self {
            fd: libc::AT_FDCWD,
            t: IoType::Timeout(data::IoTimeoutData::new(duration)),
            duration: IoDuration::Single,
        }
    }

    /*

        Synchronization operations
    
    */

    fn _sync<T: AsRawFd>(
        fd: &T,
        duration: IoDuration,
    ) -> Self {
        Self {
            fd: fd.as_raw_fd(),
            t: IoType::FSync(data::IoFSyncData {}),
            duration
        }
    }

    pub fn sync<T: AsRawFd>(fd: &T) -> Self {
        Self::_sync(fd, IoDuration::Single)
    }

    pub fn sync_forget<T: AsRawFd>(fd: &T) -> Self {
        Self::_sync(fd, IoDuration::Zero)
    }

    fn _datasync<T: AsRawFd>(
        fd: &T,
        duration: IoDuration,
    ) -> Self {
        Self {
            fd: fd.as_raw_fd(),
            t: IoType::Fdatasync(data::IoFDatasyncData {}),
            duration
        }
    }

    pub fn datasync<T: AsRawFd>(fd: &T) -> Self {
        Self::_datasync(fd, IoDuration::Single)
    }

    pub fn datasync_forget<T: AsRawFd>(fd: &T) -> Self {
        Self::_datasync(fd, IoDuration::Zero)
    }
    

    // Complete the operation. 
    // In general, oneshot type operations can only ve validly completed once.
    // Persistent operations can be completed multiple times.
    pub (crate) fn complete(
        &mut self, 
        result_code: i32
    ) -> IoCompletionResult {
        if result_code < 0 {
            let os_error = OsError::from(-result_code).into();
            let failure = self.t.get_failure();
            Err(super::IoOperationError { 
                failure, 
                os_error
            })
        } else {
            Ok(self.t.get_completion(result_code as u32))
        }
    }

    pub (crate) fn as_uring_entry(
        &mut self, 
        key: Key
    ) -> io_uring::squeue::Entry {
        let fd = self.fd;
        self.t.as_uring_entry(fd, key)
    }
}

impl AsRawFd for IoOperation {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl IoBytesMutRecovery for IoOperation {
    fn as_bytes_mut(&self) -> Option<&BytesMut> {
        self.t.as_bytes_mut()
    }

    fn into_bytes_mut(self) -> Option<BytesMut> {
        self.t.into_bytes_mut()
    }
    
    fn take_bytes_mut(&mut self) -> Option<BytesMut> {
        self.t.take_bytes_mut()
    }
}

impl IoBytesMutVecRecovery for IoOperation {
    fn as_vec(&self) -> Option<&Vec<BytesMut>> {
        self.t.as_vec()
    }

    fn into_vec(self) -> Option<Vec<BytesMut>> {
        self.t.into_vec()
    }
    
    fn take_vec(&mut self) -> Option<Vec<BytesMut>> {
        self.t.take_vec()
    }
}

impl IoRecvMessageRecovery for IoOperation {
    fn as_recvmsg_buffers(&self) -> Option<RecvMsgBuffersRefs<'_>> {
        self.t.as_recvmsg_buffers()
    }

    fn into_recvmsg_buffers(self) -> Option<RecvMsgBuffers> {
        self.t.into_recvmsg_buffers()
    }
    
    fn take_recvmsg_buffers(&mut self) -> Option<RecvMsgBuffers> {
        self.t.take_recvmsg_buffers()
    }
}