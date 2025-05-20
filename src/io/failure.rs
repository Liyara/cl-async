

pub mod data {
    use bytes::BytesMut;

    use crate::io::RecvMsgBuffers;

    #[derive(Debug)]
    pub struct IoReadFailure {
        pub buffer: BytesMut
    }

    #[derive(Debug)]
    pub struct IoMultiReadFailure {
        pub buffers: Vec<BytesMut>,
    }

    #[derive(Debug)]
    pub struct IoMsgFailure {
        pub buffers: RecvMsgBuffers,
    }
}

#[derive(Debug)]
pub enum IoFailure {
    Generic,
    Read(data::IoReadFailure),
    MultiRead(data::IoMultiReadFailure),
    Msg(data::IoMsgFailure),
}