# mux server: do not replace a socket another server still listens on

`safely_create_sock_path()` removes whatever file is at the unix domain's
socket path before it binds. A server started with `--daemonize` is protected
by the pid file lock, but one started in the foreground (by hand, or under a
supervisor) takes no lock, so a second server on the same path deletes a live
server's socket and binds its own. The first server keeps running with all of
its panes, and no new client can reach it.

Before removing the file, try a nonblocking connect to it. If something
accepts, fail with "<path> is in use by another server; not replacing it" and
leave the file alone. A refused connection or a missing file means a stale
socket, which is removed and replaced as before. If the probe cannot be made
at all (no socket can be opened, or the path is too long), the start fails
with that error, since binding would fail the same way. The connect does not
block, so a server whose backlog is full cannot stall startup: Linux reports
that as `EAGAIN`, which counts as listening; macOS refuses the connection just
as it does for a stale socket, so there a server that has stopped accepting is
still replaced, as it is today. The check is unix only; Windows is unchanged.

`spawn_listener` now binds each distinct socket path once. Two unix domains
that name the same socket (for instance two that both leave `socket_path` at
its default) used to bind it twice, the second replacing the first; with the
probe, the second would find the first and stop the server from starting.

Not covered: the probe and the bind are separate steps, so two foreground
servers started at the same instant can both pass the probe. Only the pid file
lock of `--daemonize` covers that.

Validation on the unit branch:

- `cargo fmt --all -- --check` passes. `cargo test -p wezterm-mux-server-impl`:
  the new `a_second_server_leaves_a_live_socket_alone` passes, and fails with
  "a second server took the path" when the probe is disabled.
- Two servers started in the foreground on one throwaway socket, with every
  pane running `sleep`. Before: the second replaced the socket (its inode
  changed) while the first kept running, unreachable. After: the second exited
  with the error above, the inode was unchanged, and `wezterm cli list` reached
  the first.
- One server whose config names the same socket in two unix domains: it starts
  and answers `wezterm cli list` before and after. A version of this change
  without the `spawn_listener` part exited with "problem spawning listeners".
- No test fills a listener's backlog on Linux; the `EAGAIN` handling there is
  from connect(2), not from a run.

An AI coding agent wrote the change and ran the validation. Nothing was tested
by hand. No upstream submission has been made.
