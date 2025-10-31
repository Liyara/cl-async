use std::os::fd::RawFd;
use bitflags::bitflags;
use bytes::BytesMut;

use crate::{io::{buffers::{IoRecvMsgOutputBuffers, RecvMsgBuffersRefs}, message::{IoRecvMessage, PendingIoMessage}, IoBytesMutRecovery, IoBytesMutVecRecovery, IoRecvMessageRecovery, IoSubmissionError, RecvMsgBuffers}, Key};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IoRecvMsgInputFlags: i32 {
        const TRUNCATE = libc::MSG_TRUNC;
        const OUT_OF_BAND = libc::MSG_OOB;
        const ERR_QUEUE = libc::MSG_ERRQUEUE;
        const PEEK = libc::MSG_PEEK;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IoRecvMsgOutputFlags: i32 {
        const END_OF_RECORD = libc::MSG_EOR;
        const TRUNCATED = libc::MSG_TRUNC;
        const CONTROL_TRUNCATED = libc::MSG_CTRUNC;
        const OUT_OF_BAND = libc::MSG_OOB;
        const ERR_QUEUE = libc::MSG_ERRQUEUE;
    }
}

pub struct IoRecvMsgData {
    pub flags: IoRecvMsgInputFlags,
    pending_msg: Option<PendingIoMessage>,
}

impl IoRecvMsgData {
    pub fn new(
        buffers: IoRecvMsgOutputBuffers,
        flags: IoRecvMsgInputFlags,
    ) -> Result<Self, IoSubmissionError> {

        Ok(
            Self {
                flags,
                pending_msg: Some(PendingIoMessage::new(
                    buffers,
                )?),
            }
        )
    }
}

impl super::CompletableOperation for IoRecvMsgData {
    fn get_completion(&mut self, result_code: u32) -> crate::io::IoCompletion {

        if self.pending_msg.is_none() {
            warn!("cl-async: recvmsg(): Expected pending message but got None; returning empty message");
            return crate::io::IoCompletion::Msg(
                crate::io::completion_data::IoMsgCompletion {
                    msg: IoRecvMessage::new(
                        RecvMsgBuffers::default(),
                        None,
                        None,
                        IoRecvMsgOutputFlags::empty(),
                        0,
                        0,
                    ),
                }
            );
        }

        // Safe because we just checked is_none()
        let pending_msg = self
            .pending_msg
            .take()
            .unwrap()
            .complete(result_code as usize)
        ;

        let flags = pending_msg.parse_flags();

        let address = match pending_msg.parse_address() {
            Ok(address) => address,
            Err(e) => {
                warn!("cl-async: recvmsg(): {}", e);
                None
            }
        };

        let parsed_control = match pending_msg.parse_control() {
            Ok(control) => control,
            Err(e) => {
                warn!("cl-async: recvmsg(): {}", e);
                None
            }
        };

        let mut pending_msg = pending_msg.next();

        let buffers = match pending_msg.extract_data_buffers() {
            Ok(buffers) => buffers,
            Err(e) => {
                warn!("cl-async: recvmsg(): {}", e);
                Some(e.into_buffers())
            }
        };

        let control = match pending_msg.extract_control_buffer() {
            Ok(control) => control,
            Err(e) => {
                warn!("cl-async: recvmsg(): {}", e);
                Some(e.into_buffer())
            }
        };

        let msg = IoRecvMessage::new(
            RecvMsgBuffers::new(
                buffers,
                control,
            ),
            parsed_control,
            address,
            flags,
            pending_msg.bytes_received(),
            pending_msg.control_bytes_received(),
        );

        crate::io::IoCompletion::Msg(
            crate::io::completion_data::IoMsgCompletion { msg }
        )
    }

    fn get_failure(&mut self) -> crate::io::failure::IoFailure {

        let pending_msg = self.pending_msg.take();

        if pending_msg.is_none() {
            return crate::io::failure::IoFailure::Msg(
                crate::io::failure::data::IoMsgFailure {
                    buffers: RecvMsgBuffers::default(),
                }
            );
        }

        let pending_msg = pending_msg.unwrap();

        let (
            data_buffers, 
            control_buffer
        ) = unsafe { pending_msg.split() };

        let data_buffers = data_buffers.map(|buffers| {
            buffers.into_vec()
        });

        let control_buffer = control_buffer.map(|buffer| {
            buffer.into_bytes_unchecked()
        });

        crate::io::failure::IoFailure::Msg(
            crate::io::failure::data::IoMsgFailure {
                buffers: RecvMsgBuffers::new(
                    data_buffers,
                    control_buffer,
                ),
            }
        ) 
    }
}

impl super::AsUringEntry for IoRecvMsgData {
    
    fn as_uring_entry(&mut self, fd: RawFd, key: Key) -> io_uring::squeue::Entry {

        let pending_msg = self.pending_msg.as_mut().unwrap();

        io_uring::opcode::RecvMsg::new(
            io_uring::types::Fd(fd),
            pending_msg.as_mut_ptr() as *mut _
        ).flags(self.flags.bits() as u32)
        .build().user_data(key.as_u64())
    }
}

impl IoBytesMutRecovery for IoRecvMsgData {
    fn as_bytes_mut(&self) -> Option<&BytesMut> {
        self.pending_msg.as_ref().map(|p_msg| {
            p_msg.buffers().control.as_ref().map(|b| {
                b.as_bytes()
            })
        }).flatten()
    }

    fn into_bytes_mut(mut self) -> Option<BytesMut> {
        self.pending_msg.take().map(|p_msg| {
            unsafe { p_msg.split() }.1.map(|b| {
                b.into_bytes_unchecked()
            })
        }).flatten()
    }
    
    fn take_bytes_mut(&mut self) -> Option<BytesMut> {
        self.pending_msg.as_mut().and_then(|p_msg| {
            p_msg.buffers_mut().control.take().map(|b| {
                b.into_bytes_unchecked()
            })
        })
    }
}

impl IoBytesMutVecRecovery for IoRecvMsgData {
    fn as_vec(&self) -> Option<&Vec<BytesMut>> {
        self.pending_msg.as_ref().map(|p_msg| {
            p_msg.buffers().data.as_ref().map(|b| {
                b.as_vec()
            })
        }).flatten()
    }

    fn into_vec(mut self) -> Option<Vec<BytesMut>> {
        self.pending_msg.take().map(|p_msg| {
            unsafe { p_msg.split() }.0.map(|b| {
                b.into_vec()
            })
        }).flatten()
    }
    
    fn take_vec(&mut self) -> Option<Vec<BytesMut>> {
        self.pending_msg.as_mut().and_then(|p_msg| {
            p_msg.buffers_mut().data.take().map(|b| {
                b.into_vec()
            })
        })
    }
}

impl IoRecvMessageRecovery for IoRecvMsgData {
    fn as_recvmsg_buffers(&self) -> Option<RecvMsgBuffersRefs<'_>> {
        self.pending_msg.as_ref().map(|p_msg| {
            p_msg.buffers().as_raw()
        })
    }

    fn into_recvmsg_buffers(mut self) -> Option<RecvMsgBuffers> {
        self.pending_msg.take().map(|p_msg| {
            unsafe { p_msg.into_buffers() }.unwrap()
        })
    }
    
    fn take_recvmsg_buffers(&mut self) -> Option<RecvMsgBuffers> {
        self.pending_msg.as_mut().and_then(|p_msg| {
            let bufs = p_msg.buffers_mut();
            Some(RecvMsgBuffers::new(
                bufs.data.take().map(|b| b.into_vec()),
                bufs.control.take().map(|b| b.into_bytes_unchecked()),
            ))
        })
    }
}