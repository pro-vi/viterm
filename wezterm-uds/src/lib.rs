use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, RawFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream as StreamImpl;
#[cfg(windows)]
use std::os::windows::io::{
    AsRawSocket, AsSocket, BorrowedSocket, FromRawSocket, IntoRawSocket, RawSocket,
};
use std::path::Path;
#[cfg(windows)]
use uds_windows::UnixStream as StreamImpl;

#[cfg(unix)]
use std::os::unix::net::UnixListener as ListenerImpl;
#[cfg(windows)]
use uds_windows::UnixListener as ListenerImpl;

#[cfg(unix)]
use std::os::unix::net::SocketAddr;
#[cfg(windows)]
use uds_windows::SocketAddr;

/// This wrapper makes UnixStream IoSafe on all platforms.
/// This isn't strictly needed on unix, because async-io
/// includes an impl for the std UnixStream, but on Windows
/// the uds_windows crate doesn't have an impl.
/// Here we define it for all platforms in the interest of
/// minimizing platform differences.
#[derive(Debug)]
pub struct UnixStream(StreamImpl);

#[cfg(unix)]
impl AsFd for UnixStream {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
#[cfg(unix)]
impl IntoRawFd for UnixStream {
    fn into_raw_fd(self) -> RawFd {
        self.0.into_raw_fd()
    }
}
#[cfg(unix)]
impl FromRawFd for UnixStream {
    unsafe fn from_raw_fd(fd: RawFd) -> UnixStream {
        UnixStream(StreamImpl::from_raw_fd(fd))
    }
}
#[cfg(unix)]
impl AsRawFd for UnixStream {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

#[cfg(windows)]
impl IntoRawSocket for UnixStream {
    fn into_raw_socket(self) -> RawSocket {
        self.0.into_raw_socket()
    }
}
#[cfg(windows)]
impl AsRawSocket for UnixStream {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.as_raw_socket()
    }
}
#[cfg(windows)]
impl AsSocket for UnixStream {
    fn as_socket(&self) -> BorrowedSocket {
        self.0.as_socket()
    }
}
#[cfg(windows)]
impl FromRawSocket for UnixStream {
    unsafe fn from_raw_socket(socket: RawSocket) -> UnixStream {
        UnixStream(StreamImpl::from_raw_socket(socket))
    }
}

impl Read for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.0.read(buf)
    }
}

impl Write for UnixStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> Result<(), std::io::Error> {
        self.0.flush()
    }
}

unsafe impl async_io::IoSafe for UnixStream {}

/// Socket buffer size we ask the kernel for on the mux protocol connection.
///
/// `mux::allocate_socketpair` already raises the pty socketpairs to 1 MiB, but
/// the unix domain socket carrying the mux protocol itself was never raised, so
/// it inherited the OS default. On macOS that default is 8 KiB
/// (`net.local.stream.sendspace` / `recvspace`), and attaching a GUI to a
/// server with many panes deadlocks: both sides write large payloads while
/// neither is reading, both buffers fill, and the connection wedges.
///
/// Overridable via `WEZTERM_UDS_BUFSIZE` so the failure can be reproduced
/// without changing a machine-wide sysctl.
#[cfg(unix)]
const DEFAULT_BUFSIZE: usize = 1024 * 1024;

#[cfg(unix)]
fn desired_bufsize() -> usize {
    match std::env::var("WEZTERM_UDS_BUFSIZE") {
        Ok(v) => v.parse().unwrap_or(DEFAULT_BUFSIZE),
        Err(_) => DEFAULT_BUFSIZE,
    }
}

impl UnixStream {
    pub fn connect<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        Ok(Self::tuned(StreamImpl::connect(path)?))
    }

    /// Wrap a stream and raise its socket buffers, best-effort.
    ///
    /// Failure is deliberately ignored: this is purely a throughput
    /// consideration, and a system whose cap is below what we ask for must
    /// still be able to connect. Same posture as the resolution of #6712.
    fn tuned(stream: StreamImpl) -> Self {
        let me = Self(stream);
        #[cfg(unix)]
        me.set_socket_buffers(desired_bufsize());
        me
    }

    #[cfg(unix)]
    fn set_socket_buffers(&self, size: usize) {
        let size = size as libc::c_int;
        let socklen = std::mem::size_of_val(&size) as libc::socklen_t;
        for option in [libc::SO_SNDBUF, libc::SO_RCVBUF] {
            unsafe {
                libc::setsockopt(
                    self.as_raw_fd(),
                    libc::SOL_SOCKET,
                    option,
                    &size as *const libc::c_int as *const _,
                    socklen,
                );
            }
        }
        // Investigation aid, not for the eventual patch: read back what the
        // kernel actually granted, so a negative reproduction cannot be
        // confused with "the setsockopt never happened".
        if std::env::var("WEZTERM_UDS_DEBUG").is_ok() {
            let mut got: libc::c_int = 0;
            let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
            let mut readback = [0i32; 2];
            for (i, option) in [libc::SO_SNDBUF, libc::SO_RCVBUF].iter().enumerate() {
                unsafe {
                    libc::getsockopt(
                        self.as_raw_fd(),
                        libc::SOL_SOCKET,
                        *option,
                        &mut got as *mut libc::c_int as *mut _,
                        &mut len,
                    );
                }
                readback[i] = got;
            }
            eprintln!(
                "wezterm-uds: fd {} asked {} -> snd {} rcv {}",
                self.as_raw_fd(), size, readback[0], readback[1]
            );
        }
    }
}

impl std::ops::Deref for UnixStream {
    type Target = StreamImpl;
    fn deref(&self) -> &StreamImpl {
        &self.0
    }
}

impl std::ops::DerefMut for UnixStream {
    fn deref_mut(&mut self) -> &mut StreamImpl {
        &mut self.0
    }
}

pub struct UnixListener(ListenerImpl);

impl UnixListener {
    pub fn bind<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        Ok(Self(ListenerImpl::bind(path)?))
    }

    pub fn accept(&self) -> std::io::Result<(UnixStream, SocketAddr)> {
        let (stream, addr) = self.0.accept()?;
        Ok((UnixStream::tuned(stream), addr))
    }

    pub fn incoming(&self) -> impl Iterator<Item = std::io::Result<UnixStream>> + '_ {
        self.0.incoming().map(|r| r.map(UnixStream::tuned))
    }
}

impl std::ops::Deref for UnixListener {
    type Target = ListenerImpl;
    fn deref(&self) -> &ListenerImpl {
        &self.0
    }
}

impl std::ops::DerefMut for UnixListener {
    fn deref_mut(&mut self) -> &mut ListenerImpl {
        &mut self.0
    }
}
