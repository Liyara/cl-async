use std::fmt;


#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Hangup,
    Interrupt,
    Quit,
    Kill,
    Trap,
    Abort,
    BusError,
    FloatingPointError,
    SegmentationFault,
    BrokenPipe,
    Alarm,
    Terminate,
    ChildStopped,
    Continue,
    Stop,
    TerminalStop,
    TtyIn,
    TtyOut,
    UserDefined(libc::c_int),
    Unknown(libc::c_int),
}

impl From<libc::c_int> for Signal {
    fn from(signal: libc::c_int) -> Self {
        match signal {
            libc::SIGHUP => Signal::Hangup,
            libc::SIGINT => Signal::Interrupt,
            libc::SIGQUIT => Signal::Quit,
            libc::SIGKILL => Signal::Kill,
            libc::SIGTRAP => Signal::Trap,
            libc::SIGABRT => Signal::Abort,
            libc::SIGBUS => Signal::BusError,
            libc::SIGFPE => Signal::FloatingPointError,
            libc::SIGSEGV => Signal::SegmentationFault,
            libc::SIGPIPE => Signal::BrokenPipe,
            libc::SIGALRM => Signal::Alarm,
            libc::SIGTERM => Signal::Terminate,
            libc::SIGCHLD => Signal::ChildStopped,
            libc::SIGCONT => Signal::Continue,
            libc::SIGSTOP => Signal::Stop,
            libc::SIGTSTP => Signal::TerminalStop,
            libc::SIGTTIN => Signal::TtyIn,
            libc::SIGTTOU => Signal::TtyOut,
            libc::SIGUSR1 => Signal::UserDefined(libc::SIGUSR1),
            libc::SIGUSR2 => Signal::UserDefined(libc::SIGUSR2),
            _ => Signal::Unknown(signal)
        }
    }
}

impl Into<libc::c_int> for Signal {
    fn into(self) -> libc::c_int {
        match self {
            Signal::Hangup => libc::SIGHUP,
            Signal::Interrupt => libc::SIGINT,
            Signal::Quit => libc::SIGQUIT,
            Signal::Kill => libc::SIGKILL,
            Signal::Trap => libc::SIGTRAP,
            Signal::Abort => libc::SIGABRT,
            Signal::BusError => libc::SIGBUS,
            Signal::FloatingPointError => libc::SIGFPE,
            Signal::SegmentationFault => libc::SIGSEGV,
            Signal::BrokenPipe => libc::SIGPIPE,
            Signal::Alarm => libc::SIGALRM,
            Signal::Terminate => libc::SIGTERM,
            Signal::ChildStopped => libc::SIGCHLD,
            Signal::Continue => libc::SIGCONT,
            Signal::Stop => libc::SIGSTOP,
            Signal::TerminalStop => libc::SIGTSTP,
            Signal::TtyIn => libc::SIGTTIN,
            Signal::TtyOut => libc::SIGTTOU,
            Signal::UserDefined(signal) => signal,
            Signal::Unknown(signal) => signal
        }
    }
}

impl Signal {


    // These are signals we might reasonable use custom handling logic for
    pub fn handleable() -> impl Iterator<Item = Signal> {
        [
            Signal::Hangup,
            Signal::Interrupt,
            Signal::Quit,
            Signal::Alarm,
            Signal::Terminate,
            Signal::ChildStopped,
            Signal::Continue,
            Signal::TtyIn,
            Signal::TtyOut,
            Signal::UserDefined(libc::SIGUSR1),
            Signal::UserDefined(libc::SIGUSR2),
        ]
        .iter()
        .copied()
    }
}

impl fmt::Display for Signal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Signal::Hangup => write!(f, "SIGHUP"),
            Signal::Interrupt => write!(f, "SIGINT"),
            Signal::Quit => write!(f, "SIGQUIT"),
            Signal::Kill => write!(f, "SIGKILL"),
            Signal::Trap => write!(f, "SIGTRAP"),
            Signal::Abort => write!(f, "SIGABRT"),
            Signal::BusError => write!(f, "SIGBUS"),
            Signal::FloatingPointError => write!(f, "SIGFPE"),
            Signal::SegmentationFault => write!(f, "SIGSEGV"),
            Signal::BrokenPipe => write!(f, "SIGPIPE"),
            Signal::Alarm => write!(f, "SIGALRM"),
            Signal::Terminate => write!(f, "SIGTERM"),
            Signal::ChildStopped => write!(f, "SIGCHLD"),
            Signal::Continue => write!(f, "SIGCONT"),
            Signal::Stop => write!(f, "SIGSTOP"),
            Signal::TerminalStop => write!(f, "SIGTSTP"),
            Signal::TtyIn => write!(f, "SIGTTIN"),
            Signal::TtyOut => write!(f, "SIGTTOU"),
            Signal::UserDefined(signal) => write!(f, "SIGUSR ({})", signal),
            Signal::Unknown(signal) => write!(f, "Unknown signal: {}", signal)
        }?;
        let code: libc::c_int = (*self).into();
        write!(f, " ({})", code)
    }
}