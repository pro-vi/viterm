# Index unchanged terminal scrollback for dirty-row queries

A mux render query examined every retained line to find changes since a
client sequence number. With long scrollback, unchanged history was visited
again for each output event and render client.

Screen now caches stable-row block summaries of actual line sequence numbers.
An ordered maximum-sequence index selects candidate blocks, and the existing
Line::changed_since predicate still decides each returned row. Sequence-zero
lines remain candidates. Mutable row access invalidates summaries before
borrowing; scrolling, eviction, margin operations, replacement and reflow
invalidate affected membership or rebuild. Pending invalidation is clipped
to cached blocks, including when no render queries occur.

Validation on independent unit `69c67ec5e`:

- All 61 wezterm-term tests, four mux tests and the full formatting check pass.
- Eight new tests cover inspected-row cost and full-scan parity across mutation,
  scrolling, zero/lower-sequence replacement, expanded logical callbacks,
  margins, wrapped storage, eviction and nonblank reflow.
- A warm single-row update with 200,000 history rows inspects at most 128
  rows; a subsequent unchanged query inspects zero. Restoring a full scan or
  omitting zero-sequence candidates makes the respective cost/parity test
  fail. Removing mutable-line invalidation fails three corresponding tests.
  Two partial-scroll regressions were observed failing before repair.
- Unit binary `20261004-103028-69c67ec5`, one raw render client, 200 writes
  at 20 Hz per case: initially empty history used 0.28/0.28/0.29 CPU seconds;
  200,000 initial rows used 0.31/0.32/0.31 seconds. Each case delivered 201
  render pushes. The connection receives output without explicit subscription
  because this independent unit contains no subscription change.
- A render subscriber received both foreground and unpolled background-pane
  deltas. One hundred closed clients returned to baseline descriptors on the
  disposable server.

Cold queries and rebuilds still inspect retained rows. Additional index
memory and cold-query latency have not been isolated experimentally. No
render protocol or Lua configuration option changes.

An AI coding agent authored and measured the change using a minimal config
and synthetic text on disposable Unix sockets. Manual GUI validation has not
been performed. No upstream submission has been made.
