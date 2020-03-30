macro_rules! size_of {
    ($t: ty) => {
        ::core::mem::size_of::<$t>()
    };
}

macro_rules! align_of {
    ($t: ty) => {
        ::core::mem::align_of::<$t>()
    };
}

mod aligned_buffer;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(all(unix, not(target_os = "linux")))]
mod bsd;

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    Desync,
}

type Result<A> = core::result::Result<A, Error>;

#[cfg(unix)]
use fd::Fd as RoutingSocket;
#[cfg(unix)]
mod fd;