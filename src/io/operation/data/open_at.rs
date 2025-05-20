use std::{ffi::CString, fmt, os::{fd::RawFd, unix::ffi::OsStrExt}, path::Path};
use bitflags::bitflags;
use bytes::Bytes;

use crate::io::IoSubmissionError;

bitflags! {
    #[derive(Default, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct IoFileSystemPermissions: u32 {
        const NONE = 0;
        const EXECUTE = 1 << 0;
        const WRITE = 1 << 1;
        const READ = 1 << 2;
    }
}

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

impl Default for IoFileCreateMode {
    fn default() -> Self {
        Self::DoNotCreate
    }
}

#[derive(Debug, Clone)]
pub struct IoFileOpenSettings {
    flags: i32,
    mode: IoFileCreateMode,
}

impl fmt::Display for IoFileOpenSettings {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "IoFileOpenSettings {{ flags: {}, mode: {:?} }}", self.flags, self.mode)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(i32)]
pub enum IoFileWriteMode {
    Default = 0,
    Append = libc::O_APPEND,
    Truncate = libc::O_TRUNC,
}

impl IoFileOpenSettings {

    pub fn dir() -> Self {
        Self { 
            flags: libc::O_RDONLY | libc::O_DIRECTORY,
            mode: IoFileCreateMode::DoNotCreate
        }
    }

    pub fn read_only(mode: IoFileCreateMode) -> Self {
        Self { 
            flags: libc::O_RDONLY | mode.as_flag(),
            mode
        }
    }

    pub fn write_only(
        write_mode: IoFileWriteMode,
        mode: IoFileCreateMode
    ) -> Self {
        Self { 
            flags: libc::O_WRONLY | write_mode as i32 | mode.as_flag(),
            mode
        }
    }

    pub fn read_write(
        write_mode: IoFileWriteMode,
        mode: IoFileCreateMode
    ) -> Self {
        Self { 
            flags: libc::O_RDWR | write_mode as i32 | mode.as_flag(),
            mode
        }
    }

    pub fn is_dir(&self) -> bool {
        self.flags & libc::O_DIRECTORY != 0
    }

    pub fn is_read_only(&self) -> bool {
        self.flags & libc::O_RDONLY != 0
    }

    pub fn is_write_only(&self) -> bool {
        self.flags & libc::O_WRONLY != 0
    }

    pub fn is_read_write(&self) -> bool {
        self.flags & libc::O_RDWR != 0
    }

    pub fn mode(&self) -> &IoFileCreateMode {
        &self.mode
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
        let mut op = io_uring::opcode::OpenAt::new(
            io_uring::types::Fd(fd),
            self.path.as_ref().unwrap().as_ptr(),
        ).flags(self.settings.flags);

        if let IoFileCreateMode::Create(mode) = self.settings.mode {
            op = op.mode(u32::from(mode))
        };
        
        op.build().user_data(key.as_u64())
    }
}