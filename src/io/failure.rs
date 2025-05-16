

pub mod data {
    use bytes::BytesMut;

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
        pub data_buffers: Option<Vec<BytesMut>>,
        pub control_buffer: Option<BytesMut>,
    }
}

#[derive(Debug)]
pub enum IoFailure {
    Generic,
    Read(data::IoReadFailure),
    MultiRead(data::IoMultiReadFailure),
    Msg(data::IoMsgFailure),
}