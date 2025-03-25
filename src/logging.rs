
#[cfg(feature = "log-info")]
macro_rules! info {
    ($($arg:tt)+) => (log::info!($($arg)+))
}

#[cfg(not(feature = "log-info"))]
macro_rules! info {
    ($($arg:tt)+) => {{
        if false {
            let _ = format_args!($($arg)+);
        }
    }};
}

#[cfg(feature = "log-error")]
macro_rules! error {
    ($($arg:tt)+) => (log::error!($($arg)+))
}

#[cfg(not(feature = "log-error"))]
macro_rules! error {
    ($($arg:tt)+) => {{
        if false {
            let _ = format_args!($($arg)+);
        }
    }};
}


#[cfg(feature = "log-warn")]
macro_rules! warn {
    ($($arg:tt)+) => (log::warn!($($arg)+))
}

#[cfg(not(feature = "log-warn"))]
macro_rules! warn {
    ($($arg:tt)+) => {{
        if false {
            let _ = format_args!($($arg)+);
        }
    }};
}

