use std::os::fd::RawFd;

use crate::{net::{PeerAddress, SocketAddress}, Key};

pub struct IoAcceptData {
    addr: Box<libc::sockaddr_storage>,
    addr_len: Box<libc::socklen_t>
}

impl IoAcceptData {
    pub fn new() -> Self {
        Self {
            addr: Box::new(unsafe { std::mem::zeroed() }),
            addr_len: Box::new(std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t)
        }
    }
}

impl super::CompletableOperation for IoAcceptData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletionResult {
        Ok(
            crate::io::IoCompletion::Accept(
                crate::io::completion_data::IoAcceptCompletion {
                    fd: result_code as RawFd,
                    address: Some(PeerAddress::from(
                        SocketAddress::try_from(*self.addr)?
                    ))
                }
            )
        )
    }
}

impl super::AsUringEntry for IoAcceptData {
    fn as_uring_entry(&mut self, fd: RawFd, key: Key) -> io_uring::squeue::Entry {
        io_uring::opcode::Accept::new(
            io_uring::types::Fd(fd),
            self.addr.as_mut() as *mut _ as *mut libc::sockaddr,
            self.addr_len.as_mut() as *mut libc::socklen_t
        ).build().user_data(key.as_u64())
    }
}