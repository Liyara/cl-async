
use cl_async::io::fs::futures::{FileOperationResult, JoinableFileFuture};
use cl_log::{level::Level, write_options::WriteOptions, Logger};
use log::{error, trace};

/*const HTTP_RESP: &[u8] = b"HTTP/1.1 200 OK\r\n\
Content-Type: text/html\r\n\
Content-Length: 5\r\n\
\r\n\
Hello";

#[derive(Debug, PartialEq, Eq)]
enum ConnectionState {
    Reading,
    Writing,
    Done,
}

pub struct RequestContext {
    pub stream: std::net::TcpStream,
    pub content_length: usize,
    pub write_offset: usize,
    pub buf: Vec<u8>,
    state: ConnectionState,
}

pub struct ListenerHandler {
    pub listener: std::net::TcpListener,
}

impl ListenerHandler {
    pub fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:8080").expect("Failed to bind to port 8080");
        listener.set_nonblocking(true).expect("Failed to set non-blocking");
        Self { listener }
    }
}

impl cl_async::EventHandler for ListenerHandler {
    fn handle(&self, _: cl_async::Event, _: cl_async::SubscriptionHandle) -> Option<cl_async::Task> {
        let mut fds = Vec::new();
        
        loop {
            match self.listener.accept() {
                Ok((stream, addr)) => {
                    info!("Accepted connection from {}", addr);
                    stream.set_nonblocking(true).expect("Failed to set non-blocking");
                    let fd = stream.as_raw_fd();
                    fds.push((fd, ConnectionHandler::new(stream)));
                },
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                    break;
                }
            }
        }
        
        Some(cl_async::Task::new(async move {
            for (fd, handler) in fds {
                cl_async::register_interest(
                    &fd,
                    cl_async::InterestType::READ,
                    handler,
                ).expect("Failed to register interest");
            }
        }))
    }
}

pub struct ConnectionHandler {
    context: Arc<Mutex<RequestContext>>,
}

impl ConnectionHandler {
    pub fn new(stream: std::net::TcpStream) -> Self {
        Self {
            context: Arc::new(Mutex::new(RequestContext::new(stream))),
        }
    }
}

pub async fn handle_request(
    context: Arc<Mutex<RequestContext>>,
    handle: cl_async::SubscriptionHandle,
    event: cl_async::Event
) {
    let mut ctx = context.lock();

    match ctx.state {
        ConnectionState::Reading if event.is_readable() => {
            if let Err(e) = ctx.read_cb() {
                error!("Read error: {}", e);
                ctx.state = ConnectionState::Done;
            } else {
                ctx.state = ConnectionState::Writing;
                handle.modify(cl_async::InterestType::WRITE);
            }
        }
        
        ConnectionState::Writing if event.is_writable() => {
            let remaining = ctx.write_cb();
            
            if remaining == 0 {
                ctx.state = ConnectionState::Done;
            } else {
                // Maintain WRITE interest until fully written
                handle.modify(cl_async::InterestType::WRITE);
            }
        }
        
        _ => {}
    }

    if ctx.state == ConnectionState::Done {
        handle.deregister();
        match ctx.stream.shutdown(std::net::Shutdown::Both) {
            Ok(_) => info!("Connection closed"),
            Err(e) if e.kind() == std::io::ErrorKind::NotConnected => {},
            Err(e) => error!("Failed to shutdown connection: {}", e),
        }
    }
}

impl cl_async::EventHandler for ConnectionHandler {
    fn handle(
        &self,
        _event: cl_async::Event,
        data: cl_async::SubscriptionHandle
    ) -> Option<cl_async::Task> {
        Some(cl_async::Task::new(handle_request(
            self.context.clone(), 
            data, 
            _event
        )))
    }
}

// Simplified request context
impl RequestContext {

    pub fn new(stream: std::net::TcpStream) -> Self {
        Self {
            stream,
            content_length: 0,
            buf: Vec::new(),
            state: ConnectionState::Reading,
            write_offset: 0,
        }
    }

    pub fn read_cb(&mut self) -> std::io::Result<()> {
        let mut total_read = 0;
        let mut buf = [0u8; 8192];
        
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => break, // Connection closed
                Ok(n) => {
                    total_read += n;
                    self.buf.extend_from_slice(&buf[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        
        // Only switch to write interest after full request is read
        if total_read > 0 {
            self.content_length = total_read;
        }
        Ok(())
    }

    pub fn write_cb(&mut self) -> usize {
        let remaining = HTTP_RESP.len() - self.write_offset;
        if remaining == 0 {
            return 0;
        }
        
        match self.stream.write(&HTTP_RESP[self.write_offset..]) {
            Ok(n) => {
                self.write_offset += n;
                HTTP_RESP.len() - self.write_offset
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => remaining,
            Err(e) => {
                error!("Write error: {}", e);
                0
            }
        }
    }
}*/

fn main() -> std::io::Result<()> {

    let opt = Some(WriteOptions::EXPANDED);
    let err_opt = Some(WriteOptions::ALL);

    match Logger::builder()
        .with_stderr(Level::Error, err_opt)
        .with_stdout(Level::Warn, opt)
        .with_stdout(Level::Info, opt)
        .with_stdout(Level::Debug, opt)
        .with_stdout(Level::Trace, opt)
    .build() {
        Ok(_) => {},
        Err(err) => {
            eprintln!("Failed to initialize logger: {}", err);
            std::process::exit(1);
        }
    }

    let file = cl_async::io::fs::File::open(
        "test.txt",
        cl_async::io::fs::AccessFlags::READ | cl_async::io::fs::AccessFlags::WRITE,
        cl_async::io::fs::CreationFlags::APPEND
    ).expect("Failed to open file");

    cl_async::spawn(async move {
        let fut = file.read_next(5)
        .expect("Failed to read from file").join(
            file.read_at(5, 5).expect("Failed to read from file")
        ).join(
            file.write(String::from("!").into_bytes()).expect("Failed to write to file")
        );
        
        let result = fut.await;
        if let Err(e) = result {
            error!("Error: {}", e);
            return;
        }

        let result = result.unwrap();

        for op_result in result {
            if let Some(FileOperationResult::Read(data)) = op_result {
                trace!("Read {} bytes: {}", data.len(), String::from_utf8_lossy(&data));
                continue;
            }
            if let Some(FileOperationResult::Write(bytes)) = op_result {
                trace!("Wrote {} bytes", bytes);
                continue;
            }
            error!("Unexpected result");
        }
    }).expect("Failed to spawn task");

    cl_async::shutdown().expect("Failed to join");

    Ok(())

}

