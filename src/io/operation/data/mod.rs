use std::os::fd::RawFd;
use enum_dispatch::enum_dispatch;
use crate::io::IoCompletionResult;
use crate::Key;

mod accept_multi;
mod accept;
mod cancel;
mod close;
mod connect;
mod fdatasync;
mod fsync;
mod mkdir;
mod open_at;
mod read;
mod readv;
mod recv_msg;
mod recv;
mod rename_at;
mod send_msg;
mod send;
mod shutdown;
mod splice;
mod statx;
mod timeout;
mod unlink_at;
mod write;
mod writev;

pub use accept_multi::IoAcceptMultiData;
pub use accept::IoAcceptData;
pub use cancel::IoCancelData;
pub use close::IoCloseData;
pub use connect::IoConnectData;
pub use fdatasync::IoFDatasyncData;
pub use fsync::IoFSyncData;
pub use mkdir::IoMkdirData;
pub use open_at::IoOpenAtData;
pub use open_at::IoFileCreateMode;
pub use open_at::IoFileOpenSettings;
pub use open_at::IoFileSystemMode;
pub use open_at::IoFileSystemPermissions;
pub use open_at::IoFileWriteMode;
pub use open_at::IoFileDescriptorType;
pub use read::IoReadData;
pub use readv::IoReadvData;
pub use recv_msg::IoRecvMsgData;
pub use recv_msg::IoRecvMsgInputFlags;
pub use recv_msg::IoRecvMsgOutputFlags;
pub use recv::IoRecvData;
pub use recv::IoRecvFlags;
pub use rename_at::IoRenameData;
pub use send_msg::IoSendMsgData;
pub use send::IoSendData;
pub use send::IoSendFlags;
pub use shutdown::IoShutdownData;
pub use shutdown::IoShutdownType;
pub use splice::IoSpliceData;
pub use splice::IoSpliceFlags;
pub use statx::IoStatxData;
pub use statx::IoStatxFlags;
pub use statx::IoStatxMask;
pub use timeout::IoTimeoutData;
pub use unlink_at::IoUnlinkData;
pub use write::IoWriteData;
pub use writev::IoWritevData;

#[enum_dispatch]
pub trait CompletableOperation {
    fn get_completion(&mut self, result_code: u32) -> IoCompletionResult;
}

#[enum_dispatch]
pub trait AsUringEntry {
    fn as_uring_entry(&mut self, fd: RawFd, key: Key) -> io_uring::squeue::Entry;
}