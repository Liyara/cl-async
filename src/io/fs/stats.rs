use std::ops::Add;
use std::time::UNIX_EPOCH;

use crate::io::{
    completion::TryFromCompletion, 
    operation::future::IoOperationFuture, 
    operation_data::{
        IoFileDescriptorType, 
        IoFileSystemMode
    }, 
    IoCompletion, 
};

pub type IoStatsFuture = IoOperationFuture<crate::io::fs::Stats>;

pub struct Stats {
    pub mode: Option<IoFileSystemMode>,
    pub descriptor_type: Option<IoFileDescriptorType>,
    pub n_links: Option<u32>,
    pub user_id: Option<u32>,
    pub group_id: Option<u32>,
    pub inode: Option<u64>,
    pub size: Option<u64>,
    pub block_count: Option<u64>,
    pub block_size: Option<u32>,

    pub last_access_time: Option<std::time::SystemTime>,
    pub last_modification_time: Option<std::time::SystemTime>,
    pub last_status_change_time: Option<std::time::SystemTime>,
    pub creation_time: Option<std::time::SystemTime>,

    pub rdev_major: Option<u32>,
    pub rdev_minor: Option<u32>,

    pub device_id_major: Option<u32>,
    pub device_id_minor: Option<u32>,

    pub mount_id: Option<u64>,

    pub direct_io_memory_alignment: Option<u32>,
    pub direct_io_offset_alignment: Option<u32>,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            mode: None,
            descriptor_type: None,
            n_links: None,
            user_id: None,
            group_id: None,
            inode: None,
            size: None,
            block_count: None,
            block_size: None,

            last_access_time: None,
            last_modification_time: None,
            last_status_change_time: None,
            creation_time: None,

            rdev_major: None,
            rdev_minor: None,

            device_id_major: None,
            device_id_minor: None,

            mount_id: None,

            direct_io_memory_alignment: None,
            direct_io_offset_alignment: None
        }
    }
}

impl TryFromCompletion for Stats {
    fn try_from_completion(completion: crate::io::IoCompletion) -> Option<Self> {
        match completion {
            IoCompletion::Stats(data) => {
                Some(Stats::new(
                    data.stats,
                    &data.mask
                ))
            },
            _ => None
        }
    }
}

macro_rules! statx_mask {
    ($mask:expr, $flag:ident, $statx_field:expr) => {
        if $mask.contains(crate::io::operation::data::IoStatxMask::$flag) {
            Some($statx_field)
        } else {
            None
        }
    };
}

macro_rules! statx_mask_time {
    ($mask:expr, $flag:ident, $statx_field:expr) => {{
        let raw = statx_mask!($mask, $flag, $statx_field);

        match raw {
            Some(t) => {
                if t.tv_sec >= 0 {
                    let duration = std::time::Duration::new(
                        t.tv_sec as u64,
                        t.tv_nsec
                    );
                    UNIX_EPOCH.checked_add(duration)
                } else {
                    let secs_before_epoch = -t.tv_sec as u64;
                    let duration_to_subtract = if t.tv_nsec == 0 {
                        std::time::Duration::from_secs(secs_before_epoch)
                    } else {
                        std::time::Duration::new(secs_before_epoch - 1, 1_000_000_000 - t.tv_nsec)
                    };
                    UNIX_EPOCH.checked_sub(duration_to_subtract)
                }
            },
            None => None
        }
    }};
}

impl Stats {
    pub (crate) fn new(
        stats: libc::statx,
        mask: &crate::io::operation::data::IoStatxMask,
    ) -> Self {

        Self {
            mode: statx_mask!(mask, TYPE_OR_MODE, stats.stx_mode).map(|mode| {
                IoFileSystemMode::from(mode as libc::mode_t)
            }),
            descriptor_type: statx_mask!(mask, TYPE_OR_MODE, {
                IoFileDescriptorType::from(stats.stx_mode as libc::mode_t)
            }),
            n_links: statx_mask!(mask, NLINK, stats.stx_nlink),
            user_id: statx_mask!(mask, UID, stats.stx_uid),
            group_id: statx_mask!(mask, GID, stats.stx_gid),
            inode: statx_mask!(mask, INO, stats.stx_ino),
            size: statx_mask!(mask, SIZE, stats.stx_size),
            block_count: statx_mask!(mask, BLOCKS, stats.stx_blocks),
            block_size: statx_mask!(mask, BLOCKS, stats.stx_blksize),

            last_access_time: statx_mask_time!(mask, ATIME, stats.stx_atime),
            last_modification_time: statx_mask_time!(mask, MTIME, stats.stx_mtime),
            last_status_change_time: statx_mask_time!(mask, CTIME, stats.stx_ctime),
            creation_time: statx_mask_time!(mask, BIRTHTIME, stats.stx_btime),

            rdev_major: statx_mask!(mask, TYPE_OR_MODE, stats.stx_rdev_major),
            rdev_minor: statx_mask!(mask, TYPE_OR_MODE, stats.stx_rdev_minor),

            device_id_major: statx_mask!(mask, INO, stats.stx_dev_major),
            device_id_minor: statx_mask!(mask, INO, stats.stx_dev_minor),

            mount_id: statx_mask!(mask, MNT_ID, stats.stx_mnt_id),

            direct_io_memory_alignment: statx_mask!(mask, DIOALOGN, stats.stx_dio_mem_align),
            direct_io_offset_alignment: statx_mask!(mask, DIOALOGN, stats.stx_dio_offset_align)
        }
    }
}

impl Add for Stats {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            mode: self.mode.or(other.mode),
            descriptor_type: self.descriptor_type.or(other.descriptor_type),
            n_links: self.n_links.or(other.n_links),
            user_id: self.user_id.or(other.user_id),
            group_id: self.group_id.or(other.group_id),
            inode: self.inode.or(other.inode),
            size: self.size.or(other.size),
            block_count: self.block_count.or(other.block_count),
            block_size: self.block_size.or(other.block_size),

            last_access_time: self.last_access_time.or(other.last_access_time),
            last_modification_time: self.last_modification_time.or(other.last_modification_time),
            last_status_change_time: self.last_status_change_time.or(other.last_status_change_time),
            creation_time: self.creation_time.or(other.creation_time),

            rdev_major: self.rdev_major.or(other.rdev_major),
            rdev_minor: self.rdev_minor.or(other.rdev_minor),

            device_id_major: self.device_id_major.or(other.device_id_major),
            device_id_minor: self.device_id_minor.or(other.device_id_minor),

            mount_id: self.mount_id.or(other.mount_id),

            direct_io_memory_alignment: self.direct_io_memory_alignment.or(other.direct_io_memory_alignment),
            direct_io_offset_alignment: self.direct_io_offset_alignment.or(other.direct_io_offset_alignment)
        }
    }
}