use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path};

use bitflags::bitflags;

use crate::io::IoSubmissionError;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IoStatxFlags: i32 {
        const DEFAULT = 0;
        const AT_EMPTY_PATH = libc::AT_EMPTY_PATH;
        const AT_NO_AUTOMOUNT = libc::AT_NO_AUTOMOUNT;
        const AT_SYMLINK_NOFOLLOW = libc::AT_SYMLINK_NOFOLLOW;
        const AT_SYMLINK_FOLLOW = libc::AT_SYMLINK_FOLLOW;
        const AT_STATX_SYNC_AS_STAT = libc::AT_STATX_SYNC_AS_STAT;
        const AT_STATX_FORCE_SYNC = libc::AT_STATX_FORCE_SYNC;
        const AT_STATX_DONT_SYNC = libc::AT_STATX_DONT_SYNC;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IoStatxMask: u32 {
        const TYPE = libc::STATX_TYPE;
        const MODE = libc::STATX_MODE;
        const TYPE_OR_MODE = libc::STATX_TYPE | libc::STATX_MODE;
        const NLINK = libc::STATX_NLINK;
        const UID = libc::STATX_UID;
        const GID = libc::STATX_GID;
        const ATIME = libc::STATX_ATIME;
        const MTIME = libc::STATX_MTIME;
        const CTIME = libc::STATX_CTIME;
        const INO = libc::STATX_INO;
        const SIZE = libc::STATX_SIZE;
        const BLOCKS = libc::STATX_BLOCKS;
        const BIRTHTIME = libc::STATX_BTIME;
        const MNT_ID = libc::STATX_MNT_ID;
        const DIOALOGN = libc::STATX_DIOALIGN;

        const SIMPLE = 
            libc::STATX_TYPE |
            libc::STATX_MODE |
            libc::STATX_NLINK |
            libc::STATX_UID |
            libc::STATX_GID |
            libc::STATX_INO |
            libc::STATX_SIZE |
            libc::STATX_BLOCKS
        ;

        const TIME = 
            libc::STATX_ATIME |
            libc::STATX_MTIME |
            libc::STATX_CTIME |
            libc::STATX_BTIME
        ;

        const EXTENDED = Self::SIMPLE.bits() | Self::TIME.bits();

        const ALL = 
            libc::STATX_TYPE |
            libc::STATX_MODE |
            libc::STATX_NLINK |
            libc::STATX_UID |
            libc::STATX_GID |
            libc::STATX_ATIME |
            libc::STATX_MTIME |
            libc::STATX_CTIME |
            libc::STATX_INO |
            libc::STATX_SIZE |
            libc::STATX_BLOCKS |
            libc::STATX_BTIME |
            libc::STATX_MNT_ID |
            libc::STATX_DIOALIGN
        ;
    }
}


pub struct IoStatxData {
    path: CString,
    flags: IoStatxFlags,
    mask: IoStatxMask,
    statx: Box<libc::statx>,
}

impl IoStatxData {
    pub fn new(
        path: &Path, 
        flags: IoStatxFlags, 
        mask: IoStatxMask
    ) -> Result<Self, IoSubmissionError> {
        let path = CString::new(path.as_os_str().as_bytes())?;
        Ok(Self {
            path,
            flags,
            mask,
            statx: Box::new(unsafe { std::mem::zeroed() }),
        })
    }
}

impl super::CompletableOperation for IoStatxData {
    fn get_completion(&mut self, _: u32) -> crate::io::IoCompletion {

        crate::io::IoCompletion::Stats(crate::io::completion_data::IoStatxCompletion {
            stats: *self.statx,
            mask: self.mask,
        })
    }
}

impl super::AsUringEntry for IoStatxData {
    fn as_uring_entry(&mut self, fd: std::os::fd::RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::Statx::new(
            io_uring::types::Fd(fd),
            self.path.as_ptr(),
            self.statx.as_mut() as *mut libc::statx as *mut _,
        ).flags(self.flags.bits()).mask(self.mask.bits())
        .build().user_data(key.as_u64())
    }
}