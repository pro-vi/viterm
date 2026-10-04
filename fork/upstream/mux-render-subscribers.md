# Opt mux connections into render pushes

Every accepted mux connection previously received pane output and alert
notifications, including CLI and silent probe connections. Each output could
schedule a full render computation on the main thread for each connection.

Activate one shared connection-wide render subscription when the first
GetPaneRenderChanges arrives. Filter output and alerts before queueing them
for uninterested clients; a closed receiver still removes its mux callback.
Keep topology and RPC replies unchanged, and retain subscribed key-input echo.
Connection-wide activation preserves updates from background panes that have
not polled individually.

Validation on independent unit `e20b66bc6`:

- Four server-impl tests and `cargo fmt --all -- --check` pass.
- Removing filtering, suppressing activation and permitting silent scheduling
  makes exactly the three corresponding tests fail; restoring them is green.
- Clean binary `20261004-093514-e20b66bc`, one 80-column pane with 200,000
  history rows, 200 output writes at 20 Hz per case:

| Connected silent clients | Server CPU seconds | Render pushes |
|---|---:|---:|
| 1 | 0.06 | 0 |
| 100 | 0.10 | 0 |
| 1 | 0.10 | 0 |
| 100 | 0.10 | 0 |
| 1 | 0.09 | 0 |

Both 100-client runs fall within the repeated one-client range. A subscribed
positive control received deltas for both its requested pane and a second,
unpolled background pane. One hundred closed clients returned the server to
its seven baseline Unix descriptors in 0.013 seconds. Tests used a minimal
config and synthetic text on a disposable Unix socket.

An AI coding agent authored and measured the change. Manual GUI validation
has not been performed. No upstream submission has been made.
