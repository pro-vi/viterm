use anyhow::{anyhow, Context as _};
use config::{create_user_owned_dirs, UnixDomain};
use promise::spawn::spawn_into_main_thread;
use wezterm_uds::UnixListener;

pub struct LocalListener {
    listener: UnixListener,
}

impl LocalListener {
    pub fn new(listener: UnixListener) -> Self {
        Self { listener }
    }

    pub fn with_domain(unix_dom: &UnixDomain) -> anyhow::Result<Self> {
        let listener = safely_create_sock_path(unix_dom)?;
        Ok(Self::new(listener))
    }

    pub fn run(&mut self) {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    spawn_into_main_thread(async move {
                        crate::dispatch::process(stream).await.map_err(|e| {
                            log::error!("{:#}", e);
                            e
                        })
                    })
                    .detach();
                }
                Err(err) => {
                    log::error!("accept failed: {}", err);
                    return;
                }
            }
        }
    }
}

/// Take care when setting up the listener socket;
/// we need to be sure that the directory that we create it in
/// is owned by the user and has appropriate file permissions
/// that prevent other users from manipulating its contents.
fn safely_create_sock_path(unix_dom: &UnixDomain) -> anyhow::Result<UnixListener> {
    let sock_path = &unix_dom.socket_path();
    log::trace!("setting up {}", sock_path.display());

    let sock_dir = sock_path
        .parent()
        .ok_or_else(|| anyhow!("sock_path {} has no parent dir", sock_path.display()))?;

    create_user_owned_dirs(sock_dir)?;

    #[cfg(unix)]
    {
        use config::running_under_wsl;
        use std::os::unix::fs::PermissionsExt;

        if !running_under_wsl() && !unix_dom.skip_permissions_check {
            // Let's be sure that the ownership looks sane
            let meta = sock_dir.symlink_metadata()?;

            let permissions = meta.permissions();
            if (permissions.mode() & 0o22) != 0 {
                anyhow::bail!(
                    "The permissions for {} are insecure and currently \
                     allow other users to write to it (permissions={:?})",
                    sock_dir.display(),
                    permissions
                );
            }
        }
    }

    // Removing the socket of a server that is still running would leave
    // it running, with all of its panes, where no new client can reach it.
    #[cfg(unix)]
    if is_listening(sock_path)
        .with_context(|| format!("checking whether {} is in use", sock_path.display()))?
    {
        anyhow::bail!(
            "{} is in use by another server; not replacing it",
            sock_path.display()
        );
    }

    // We want to remove the socket if it exists.
    // However, on windows, we can't tell if the unix domain socket
    // exists using the methods on Path, so instead we just unconditionally
    // remove it and see what error occurs.
    match std::fs::remove_file(sock_path) {
        Ok(_) => {}
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => {}
            _ => return Err(err).context(format!("Unable to remove {}", sock_path.display())),
        },
    }

    let listener = UnixListener::bind(sock_path)
        .with_context(|| format!("Failed to bind to {}", sock_path.display()))?;

    config::set_sticky_bit(&sock_path);

    Ok(listener)
}

/// Whether something accepts connections on the socket at `path`.
/// An error means the probe could not be made at all, in which case
/// binding the path would fail too.
/// The connect does not block, so a server whose backlog is full cannot
/// stall the caller. A connection the system has not refused counts as
/// listening: Linux reports a full backlog as `EAGAIN` (connect(2)).
/// macOS (checked on 26.7) refuses it as it does for a socket file nobody
/// listens on, so there a server with a full backlog is taken for a stale
/// socket and replaced.
#[cfg(unix)]
fn is_listening(path: &std::path::Path) -> std::io::Result<bool> {
    use socket2::{Domain, SockAddr, Socket, Type};

    let addr = SockAddr::unix(path)?;
    let socket = Socket::new(Domain::UNIX, Type::STREAM, None)?;
    socket.set_nonblocking(true)?;
    Ok(match socket.connect(&addr) {
        Ok(()) => true,
        Err(err) => matches!(
            err.raw_os_error(),
            Some(libc::EAGAIN) | Some(libc::EINPROGRESS)
        ),
    })
}

#[cfg(all(test, unix))]
mod test {
    use super::*;

    #[test]
    fn a_second_server_leaves_a_live_socket_alone() {
        let dir = tempfile::tempdir().unwrap();
        let dom = UnixDomain {
            socket_path: Some(dir.path().join("sock")),
            ..UnixDomain::default()
        };
        let path = dom.socket_path();

        let first = safely_create_sock_path(&dom).unwrap();
        let err = safely_create_sock_path(&dom)
            .err()
            .expect("a second server took the path");
        assert!(err.to_string().contains("in use"), "{err:#}");

        // The path still reaches the first listener.
        let _client = wezterm_uds::UnixStream::connect(&path).unwrap();
        first.accept().unwrap();

        // Once nothing listens, the file left behind is stale and is
        // replaced.
        drop(first);
        assert!(path.exists());
        let _second = safely_create_sock_path(&dom).unwrap();
    }
}
