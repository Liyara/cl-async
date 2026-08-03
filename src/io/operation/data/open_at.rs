use std::{ffi::CString, os::{fd::RawFd, unix::ffi::OsStrExt}, path::Path};
use bitflags::bitflags;
use bytes::Bytes;
use io_uring::types::OpenHow;

use crate::io::IoSubmissionError;

bitflags! {
    #[derive(Default, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct IoFileDescriptorType: u32 {
        const FILE = libc::S_IFREG;
        const DIRECTORY = libc::S_IFDIR;
        const SYM_LINK = libc::S_IFLNK;
        const CHAR_DEVICE = libc::S_IFCHR;
        const BLOCK_DEVICE = libc::S_IFBLK;
        const PIPE = libc::S_IFIFO;
        const SOCKET = libc::S_IFSOCK;
        const UNKNOWN = 0;
    }
}

impl From<libc::mode_t> for IoFileDescriptorType {
    fn from(value: libc::mode_t) -> Self {
        IoFileDescriptorType::from_bits_truncate(value & libc::S_IFMT)
    }
}

impl From<libc::c_uchar> for IoFileDescriptorType {
    fn from(value: libc::c_uchar) -> Self {
        match value {
            libc::DT_BLK => IoFileDescriptorType::BLOCK_DEVICE,
            libc::DT_CHR => IoFileDescriptorType::CHAR_DEVICE,
            libc::DT_DIR => IoFileDescriptorType::DIRECTORY,
            libc::DT_FIFO => IoFileDescriptorType::PIPE,
            libc::DT_LNK => IoFileDescriptorType::SYM_LINK,
            libc::DT_REG => IoFileDescriptorType::FILE,
            libc::DT_SOCK => IoFileDescriptorType::SOCKET,
            _ => IoFileDescriptorType::UNKNOWN,
        }
    }
}

bitflags! {
    #[derive(Default, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct IoFileSystemPermissions: u32 {
        const NONE = 0;
        const EXECUTE = 1 << 0;
        const WRITE = 1 << 1;
        const READ = 1 << 2;
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(i32)]
pub enum IoFileSystemAccessType {
    ReadOnly = 0,
    WriteOnly = 1,
    ReadWrite = 2
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct IoFileSystemOpenFlags: i32 {
        const EXCLUSIVE = libc::O_EXCL;
        const DIRECTORY = libc::O_DIRECTORY;
        const NOFOLLOW = libc::O_NOFOLLOW;
        const TRUNCATE = libc::O_TRUNC;
        const TMPFILE = libc::O_TMPFILE;
        const APPEND = libc::O_APPEND;
        const ASYNC = libc::O_ASYNC;
        const CLOSE_ON_EXEC = libc::O_CLOEXEC;
        const DIRECT = libc::O_DIRECT;
        const DSYNC = libc::O_DSYNC;
        const LARGE_FILE = libc::O_LARGEFILE;
        const NO_ACCESS_TIME = libc::O_NOATIME;
        const NO_CONTROLLING_TERMINAL = libc::O_NOCTTY;
        const NONBLOCK = libc::O_NONBLOCK;
        const NO_DELAY = libc::O_NDELAY;
        const PATH = libc::O_PATH;
        const SYNC = libc::O_SYNC;
    }

}

impl IoFileSystemOpenFlags {
    pub fn to_dir_safe(self) -> Self {
        self 
        | IoFileSystemOpenFlags::DIRECTORY
        & !IoFileSystemOpenFlags::TRUNCATE
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IoFileSystemMode {
    pub user_permissions: IoFileSystemPermissions,
    pub group_permissions: IoFileSystemPermissions,
    pub other_permissions: IoFileSystemPermissions,
}

impl IoFileSystemMode {

    pub fn new(
        user_permissions: IoFileSystemPermissions,
        group_permissions: IoFileSystemPermissions,
        other_permissions: IoFileSystemPermissions,
    ) -> Self {
        Self {
            user_permissions,
            group_permissions,
            other_permissions,
        }
    }

    pub fn private() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE,
            group_permissions: IoFileSystemPermissions::NONE,
            other_permissions: IoFileSystemPermissions::NONE,
        }
    }
    pub fn private_read_only() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ,
            group_permissions: IoFileSystemPermissions::NONE,
            other_permissions: IoFileSystemPermissions::NONE,
        }
    }

    pub fn private_executable() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE | IoFileSystemPermissions::EXECUTE,
            group_permissions: IoFileSystemPermissions::NONE,
            other_permissions: IoFileSystemPermissions::NONE,
        }
    }

    pub fn private_read_executable() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::EXECUTE,
            group_permissions: IoFileSystemPermissions::NONE,
            other_permissions: IoFileSystemPermissions::NONE,
        }
    }

    pub fn shared_read_only() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE,
            group_permissions: IoFileSystemPermissions::READ,
            other_permissions: IoFileSystemPermissions::READ,
        }
    }

    pub fn shared_read_write() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE,
            group_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE,
            other_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE,
        }
    }

    pub fn shared_read_executable() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE | IoFileSystemPermissions::EXECUTE,
            group_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::EXECUTE,
            other_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::EXECUTE,
        }
    }

    pub fn group_read_only() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE,
            group_permissions: IoFileSystemPermissions::READ,
            other_permissions: IoFileSystemPermissions::NONE,
        }
    }

    pub fn group_read_write() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE,
            group_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE,
            other_permissions: IoFileSystemPermissions::NONE,
        }
    }

    pub fn group_read_executable() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE | IoFileSystemPermissions::EXECUTE,
            group_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::EXECUTE,
            other_permissions: IoFileSystemPermissions::NONE,
        }
    }

    pub fn permissive() -> Self {
        Self {
            user_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE | IoFileSystemPermissions::EXECUTE,
            group_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE | IoFileSystemPermissions::EXECUTE,
            other_permissions: IoFileSystemPermissions::READ | IoFileSystemPermissions::WRITE | IoFileSystemPermissions::EXECUTE,
        }
    }

    pub fn to_directory_safe(self) -> Self {
        Self {
            user_permissions: {
                if self.user_permissions.contains(IoFileSystemPermissions::READ) {
                    self.user_permissions | IoFileSystemPermissions::EXECUTE
                } else {
                    self.user_permissions
                }
            },
            group_permissions: {
                if self.group_permissions.contains(IoFileSystemPermissions::READ) {
                    self.group_permissions | IoFileSystemPermissions::EXECUTE
                } else {
                    self.group_permissions
                }
            },
            other_permissions: {
                if self.other_permissions.contains(IoFileSystemPermissions::READ) {
                    self.other_permissions | IoFileSystemPermissions::EXECUTE
                } else {
                    self.other_permissions
                }
            },
        }
    }
}

impl From<IoFileSystemMode> for libc::mode_t {

    fn from(value: IoFileSystemMode) -> Self {
        let user = value.user_permissions.bits() << 6;
        let group = value.group_permissions.bits() << 3;
        let other = value.other_permissions.bits();
        user | group | other
    }
}

impl From<libc::mode_t> for IoFileSystemMode {
    fn from(value: libc::mode_t) -> Self {
        let user = IoFileSystemPermissions::from_bits_truncate((value >> 6) & 0b111);
        let group = IoFileSystemPermissions::from_bits_truncate((value >> 3) & 0b111);
        let other = IoFileSystemPermissions::from_bits_truncate(value & 0b111);
        Self {
            user_permissions: user,
            group_permissions: group,
            other_permissions: other,
        }
    }
}

impl Default for IoFileSystemMode {
    fn default() -> Self {
        Self::private()
    }
}

#[derive(Debug, Clone)]
pub enum IoFileCreateMode {
    DoNotCreate,
    Create(IoFileSystemMode),
}

impl IoFileCreateMode {
    fn as_flag(&self) -> i32 {
        match self {
            IoFileCreateMode::DoNotCreate => 0,
            IoFileCreateMode::Create(_) => libc::O_CREAT
        }
    }
}

bitflags! {
    #[derive(Default, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct IoFileSystemResolveFlags: u64 {
        const NONE = 0;
        const RESOLVE_BENEATH = libc::RESOLVE_BENEATH;
        const RESOLVE_IN_ROOT = libc::RESOLVE_IN_ROOT;
        const RESOLVE_NO_SYMLINKS = libc::RESOLVE_NO_SYMLINKS;
        const RESOLVE_NO_XDEV = libc::RESOLVE_NO_XDEV;
        const RESOLVE_NO_MAGICLINKS = libc::RESOLVE_NO_MAGICLINKS;
        const RESOLVE_CACHED = libc::RESOLVE_CACHED;
    }
}

impl Default for IoFileCreateMode {
    fn default() -> Self {
        Self::DoNotCreate
    }
}

#[derive(Debug, Clone)]
pub struct IoFileOpenSettings {
    access_type: IoFileSystemAccessType,
    open_flags: IoFileSystemOpenFlags,
    resolve_flags: IoFileSystemResolveFlags,
    mode: IoFileCreateMode,
}

impl std::fmt::Display for IoFileOpenSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "IoFileOpenSettings {{ access_type: {:?}, open_flags: {:?}, resolve_flags: {:?}, mode: {:?} }}", 
            self.access_type, self.open_flags, self.resolve_flags, self.mode)
    }
}

impl IoFileOpenSettings {

    pub fn new(
        access_flags: IoFileSystemAccessType,
        open_flags: IoFileSystemOpenFlags,
        resolve_flags: IoFileSystemResolveFlags,
        mode: IoFileCreateMode
    ) -> Self {
        Self { access_type: access_flags, open_flags, resolve_flags, mode }
    }

    pub fn new_dir(
        open_flags: IoFileSystemOpenFlags,
        resolve_flags: IoFileSystemResolveFlags,
    ) -> Self {
        Self { 
            access_type: IoFileSystemAccessType::ReadOnly,
            open_flags: open_flags.to_dir_safe(),
            resolve_flags,
            mode: IoFileCreateMode::DoNotCreate
        }
    }

    pub fn as_flags(&self) -> i32 {
        self.access_type as i32 | self.open_flags.bits() | self.mode.as_flag()
    }

    pub fn is_dir(&self) -> bool {
        self.open_flags.contains(IoFileSystemOpenFlags::DIRECTORY)
    }

    pub fn is_read_only(&self) -> bool {
        matches!(self.access_type, IoFileSystemAccessType::ReadOnly)
    }

    pub fn is_write_only(&self) -> bool {
        matches!(self.access_type, IoFileSystemAccessType::WriteOnly)
    }

    pub fn is_read_write(&self) -> bool {
        matches!(self.access_type, IoFileSystemAccessType::ReadWrite)
    }

    pub fn mode(&self) -> &IoFileCreateMode {
        &self.mode
    }

    pub fn mode_value(&self) -> u32 {
        match self.mode {
            IoFileCreateMode::DoNotCreate => 0,
            IoFileCreateMode::Create(mode) => mode.into()
        }
    }

    pub fn access_type(&self) -> &IoFileSystemAccessType {
        &self.access_type
    }

    pub fn resolve_flags(&self) -> &IoFileSystemResolveFlags {
        &self.resolve_flags
    }

    pub fn open_flags(&self) -> &IoFileSystemOpenFlags {
        &self.open_flags
    }
}


pub struct IoOpenAtData {
    path: Option<CString>,
    settings: IoFileOpenSettings
}

impl IoOpenAtData {
    pub fn new(
        path: &Path,
        settings: IoFileOpenSettings
    ) -> Result<Self, IoSubmissionError> {
        Ok(Self {
            path: Some(CString::new(path.as_os_str().as_bytes())?),
            settings
        })
    }
}

impl super::CompletableOperation for IoOpenAtData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletion {

        let path = self.path.take().map(|p| {
            Bytes::from(p.into_bytes())
        }).unwrap_or_else(|| {
            warn!("cl-async: openat(): Expected path but got None; returning empty bytes.");
            Bytes::new()
        });
        
        crate::io::IoCompletion::File(
            crate::io::completion_data::IoFileCompletion { 
                fd: result_code as RawFd,
                path
            }
        )
    }
}

impl super::AsUringEntry for IoOpenAtData {
    fn as_uring_entry(&mut self, fd: RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        
        let how = OpenHow::new()
        .flags(self.settings.as_flags() as u64)
        .resolve(self.settings.resolve_flags().bits())
        .mode(self.settings.mode_value() as u64);
        
        let op = io_uring::opcode::OpenAt2::new(
            io_uring::types::Fd(fd),
            self.path.as_ref().unwrap().as_ptr(),
            &how
        );
        
        op.build().user_data(key.as_u64())
    }
}