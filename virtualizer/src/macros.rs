#[cfg(feature = "tracing")]
macro_rules! vtrace {
    ($($tt:tt)*) => {
        tracing::trace!(target: "virtualizer", $($tt)*)
    };
}

#[cfg(not(feature = "tracing"))]
macro_rules! vtrace {
    ($($tt:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! vdebug {
    ($($tt:tt)*) => {
        tracing::debug!(target: "virtualizer", $($tt)*)
    };
}

#[cfg(not(feature = "tracing"))]
macro_rules! vdebug {
    ($($tt:tt)*) => {};
}

#[cfg(feature = "tracing")]
macro_rules! vwarn {
    ($($tt:tt)*) => {
        tracing::warn!(target: "virtualizer", $($tt)*)
    };
}

#[cfg(not(feature = "tracing"))]
macro_rules! vwarn {
    ($($tt:tt)*) => {};
}
