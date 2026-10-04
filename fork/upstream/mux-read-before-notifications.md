# Stop queued mux work after a client disconnects

When pane notifications keep a client's queue ready, the mux dispatch loop
can delay reading its socket and therefore delay observing EOF. Main-thread
render closures already scheduled for that client can also outlive it.

Try a buffered read before queue receive. It preserves complete request bytes
for the existing PDU decoder and observes EOF without draining notifications.
Share session lifetime with queued render work, and check it before accessing
the mux or computing render deltas. Correlated replies and key-input echo keep
their existing meaning.

Validation on the independent unit branch:

- `cargo test --offline -p wezterm-mux-server-impl`: three tests pass.
- `cargo fmt --all -- --check`: passes with the repository's existing stable
  rustfmt warning about its nightly import-granularity option.
- Queued notifications win in the original dispatch selection; the EOF test
  fails there, including an incomplete frame followed by EOF.
- Removing the queued-work lifetime check makes only its regression fail.
- Consuming one buffered request byte makes only the byte-preservation test
  fail. Restoring both properties returns all three tests to green.

Standalone unit binary `20261004-090306-d229b4ea`, 2,000 history rows: the render subscriber received a delta for a second pane it never polled. Under continuing output, 100 connected-then-closed clients returned the server to its seven baseline Unix descriptors in 0.131 seconds. At 200,000 rows, one connected silent client received 61 unsolicited render pushes over 60 output writes; the 100-client case still exceeded the CLI deadline because render opt-in is a separate change. That run is not claimed as a passing performance result for this unit.

An AI coding agent authored the change and automated validation. Manual GUI
validation has not been performed. No upstream submission has been made.
