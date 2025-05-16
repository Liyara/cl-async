use std::os::fd::RawFd;

use crate::net::PeerAddress;

pub struct IoConnectData {
    addr: Box<libc::sockaddr_storage>,
    addr_len: libc::socklen_t,
}

impl IoConnectData {
    pub fn new(addr: PeerAddress) -> Result<Self, crate::io::IoSubmissionError> {

        let addr_storage: libc::sockaddr_storage = addr.into();

        let addr_len = match addr_storage.ss_family as libc::c_int {
            libc::AF_INET => std::mem::size_of::<libc::sockaddr_in>(),
            libc::AF_INET6 => std::mem::size_of::<libc::sockaddr_in6>(),

            // Likely impossible
            _ => return Err(
                crate::io::IoSubmissionError::UnsupportedAddressFamily(
                    addr_storage.ss_family,
                )
            ),
        } as libc::socklen_t;

        Ok(Self {
            addr: Box::new(addr_storage),
            addr_len,
        })
    }
}

impl super::CompletableOperation for IoConnectData {}

impl super::AsUringEntry for IoConnectData {
    fn as_uring_entry(&mut self, fd: RawFd, key: crate::Key) -> io_uring::squeue::Entry {
        io_uring::opcode::Connect::new(
            io_uring::types::Fd(fd),
            self.addr.as_mut() as *mut _ as *mut libc::sockaddr,
            self.addr_len as libc::socklen_t,
        ).build().user_data(key.as_u64())
    }
}