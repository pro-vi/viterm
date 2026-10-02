---
title: The GUI owns the sizes in its window; a re-read from the mux server never sends one back
objective: a mux-attached GUI settles on the window's size after any resize burst, keeps splits and zoomed panes where it put them, and does so against the mux server already running
type: fix
status: active
builder: same-session
date: 2026-10-02
origin: conversation — the ViTerm lag investigation of 2026-09-30 to 2026-10-02 (freeze on attach and resize, zoomed tabs off-size, splits off-center)
---

## Objective and background

A ViTerm GUI attached to the `human-main` mux server shows three defects, all reproduced in a
throwaway server and GUI (the server build of 2026-09-20, GUIs with and without
`unit/tabresized-on-change`):

1. **Storm.** A window resize makes the GUI rebuild the tab bar thousands of times: 34,154
   rebuilds and a 15.5-minute freeze after 5 resizes with 41 tabs. `unit/tabresized-on-change`
   cut the local share to 221; the loop that remains is wezterm/wezterm#5142.
2. **Zoomed panes keep their zoom-time size.** 4 of 4 zoomed panes stayed at 120×30 while the
   window went to about 75×21, on both engines. Live: tabs with a zoomed pane at 156×44, 173×44
   and 80×24 in a 173×43 window.
3. **Splits drift off-center.** 20 resizes 40 ms apart leave left|right splits skewed (40|35,
   41|34, 33|35) on both engines; 6 resizes 2 s apart leave 20 of 20 even. Live: every split was
   77|78 on 2026-09-27; now most are 83|89, one 93|79, two 74|98.

**Probed (traced, scratch build since removed):** in one fast burst the client sent 203 Resize
messages for one tab's two panes, 163 of them from inside a re-read and 40 from its own
layout. The last 17 seconds after the final window resize were a two-state oscillation: each
re-read sent the other of 36|37×20 (the window) and 49×25 (a stale burst size), and the tab
settled on the stale one. Conditions: debug build of upstream `b09b56c29` plus log lines,
4 split tabs. It does not establish the ordering of Resize messages in a release build.

**Why it happens.** The client has one path to the server for sizes, `ClientPane::resize`
(`wezterm-client/src/pane/clientpane.rs:398`), and five callers share it: the window layout,
split drag, zoom, pane close, and the re-read (`Tab::sync_with_pane_tree`, `mux/src/tab.rs:785`,
whose only caller is `ClientDomain::process_pane_list`, `wezterm-client/src/domain.rs:583`).
Nothing records where a size came from. The re-read already applies the active pane and zoom by
writing fields directly, without sending (`mux/src/tab.rs:793-834`); only sizes go through the
path that sends. `ClientPane::resize` decides whether to send by comparing with
`RenderableState.dimensions`, which server render updates also overwrite
(`wezterm-client/src/pane/renderable.rs:352`). Zoom adds one more stale source: the server's
`rebuild_splits_sizes_from_contained_panes` returns early for a zoomed tab, so `root_size()`
keeps reporting the split data from before the zoom.

**Settled constraints.**
- The running mux server cannot be restarted without ending every session. The client refuses
  a server whose `CODEC_VERSION` differs (`wezterm-client/src/client.rs:1165-1187`), so no codec
  change, and every fix must work against the 2026-09-20 server build.
- Each fix is a fork unit: one change per `unit/<slug>` branch, upstream-shaped, with a
  `PATCHES.md` row and a proving test (`CLAUDE.md`, "Adding a change").
- Nothing is sent upstream without the owner's go.

**Requirements.**
- R1: A size the server reported is never sent back to the server by the client.
- R2: After a resize burst ends, every tab in the window, split, single-pane or zoomed, holds
  the window's size within 10 s, and each split keeps its proportions within one cell.
- R3: The GUI with these changes attaches to and works against the 2026-09-20 server build.
- R4: One window resize step causes at most two client re-reads, whatever the pane count.
- R5: Changes made outside this GUI (a split, spawn, close or zoom from `wezterm cli` or
  another client) still appear in this GUI.

## Naming Ledger

| Role / meaning | Existing repo term | Chosen name | Owner / placement | Status | Second consumer / reason | GR6 sibling disposition |
|---|---|---|---|---|---|---|
| The last size this client asked the server to apply to a pane | none: `RenderableState.dimensions` holds both meanings | `requested_size` | `ClientPane`, `wezterm-client/src/pane/clientpane.rs` | new | written by `resize` and `adopt_server_size`, read by `resize` | `RenderableState.dimensions` keeps one meaning: the size the server last reported |
| Take a server-reported size as this pane's size without sending | none | `adopt_server_size` | `ClientPane` | new | single caller, `process_pane_list`; the reason is to make the boundary between "the GUI chose this" and "the server said this" visible at the call site | `resize` keeps the meaning "the GUI chose this size; send it if new" |
| A tab's panes and how they are split, without sizes | topology (`wezterm-client/src/client.rs:146`: "topology; we need to re-sync") | topology; predicate `is_same_topology` | `mux/src/tab.rs` | reuse | single caller, `sync_with_pane_tree`; the reason is a predicate a unit test can pin | `LayoutSnapshot` stays, for announcing `TabResized` |
| Re-reading the pane tree from the server | resync | resync | `ClientDomain::resync` | reuse | — | — |
| Whether a resync is running and whether another was asked for meanwhile | none | `ResyncState` (`Idle`, `Running`, `RunningAndQueued`) | `wezterm-client` client state | new | written by the `Pdu::TabResized` / `TabAddedToWindow` handler and by resync completion | — |

## Architecture Decision

**Approach:** the GUI process is the authority for every size in its window. A resync takes
the server's topology, active pane and zoom state, and keeps the client's own sizes for any
tab whose topology it already has. A resync never calls a pane's `resize`; a tab whose
topology changed takes the server's sizes through `adopt_server_size`, which records them
without sending. `ClientPane` sends a Resize only when the size differs from its
`requested_size`. Resyncs are coalesced to one running and at most one queued.

**Rationale:** it is the only approach that meets R3. It follows a pattern the re-read already
uses for focus and zoom (fields written directly, `mux/src/tab.rs:793-834`) and the client-side
memory pattern of the focus fix carried as `unit/pr-7871-focus-storm` (`focused_remote_pane_id`,
`wezterm-client/src/domain.rs:28`, compared in `ClientPane::advise_focus`). Everything lives in
code that runs in the GUI process (`mux` and `wezterm-client`), so it takes effect when the GUI
relaunches.

**Rejected alternative:** a size generation on the wire, in the manner of `InputSerial`
(`codec/src/lib.rs:726-758`): the client stamps each Resize and the server echoes the stamp in
`ListPanes`, so the client discards reports older than its latest request. It orders correctly
and also serves several GUIs at once, but it needs a codec change, and a `CODEC_VERSION` bump
locks the GUI out of the running server (R3). Adding the field to `ListPanesResponse` without a
bump breaks a new GUI against an old server, because the newer reader runs out of bytes
(varbincode reads by position).

**Trade-offs:**
- With two GUIs attached at different sizes, each keeps its own sizes and the server holds
  whichever asked last. Today the second GUI's request reaches the first as a report, which then
  sends it back; after this change it no longer does. Only one GUI attaches to `human-main` on
  this machine.
- Existing skew is not undone. A tab that a fresh GUI attaches to arrives with the server's
  sizes, which already carry the drift; this plan stops new drift.

**Approval criteria:**
- The resync never sends: R1 is measured by a counter that must read 0 under a burst.
- Against the 2026-09-20 server, a 20-step burst ends with every tab at the window size, splits
  within one cell of their proportions, and zoomed panes at the window size.
- No codec change.

## Contracts

### C1. Size authority
The GUI process owns the size of every tab in its windows and of every pane in those tabs. A
size reaches the server only as the GUI's choice: the window layout (`apply_dimensions`), a split
edit made in this GUI (drag, `AdjustPaneSize`), a zoom made in this GUI, or a pane close. A size
from the server updates the client only (a) as the server's report (`RenderableState.dimensions`,
render deltas, unchanged) or (b) as the sizes of a topology the client did not have, through
`adopt_server_size`.
Evidence at planning: source reading plus the trace. Risk: a sixth caller of
`ClientPane::resize` that the lanes missed would still send server sizes; the C2 counter
exposes it.

### C2. Send rule
`ClientPane::resize(size)` sends a Resize iff `size` differs from `requested_size` in cols, rows,
pixel width or pixel height, the fields the current check compares. It then sets
`requested_size` to `size`. `adopt_server_size(size)` sets `requested_size` and the reported
dimensions to `size` and sends nothing. A `ClientPane` created from a server entry starts with
`requested_size` equal to the entry's size.

### C3. Resync rule
`sync_with_pane_tree` never calls `Pane::resize`.
- Same topology as the local tree (identical split directions, identical pane in each
  leaf): keep the local tree, split sizes and `TabInner.size`; take the active pane and zoom
  state from the server. If the zoomed pane changed, re-apply the local tab size under the new
  zoom state through the normal path, so C2 decides what is sent.
- Different topology or a new tab: replace the tree with the server's, including its split
  sizes and root size, and report that the topology changed. The caller then gives each pane
  of that tab its server-reported size through `adopt_server_size`.
- Either way, announce `TabResized` only when the layout snapshot changed (the rule of
  `unit/tabresized-on-change`).

### C4. Resync coalescing
At most one resync runs per client connection. A `Pdu::TabResized` or `Pdu::TabAddedToWindow`
arriving while one runs moves the state to `RunningAndQueued`. When the running resync ends
in that state, exactly one more starts. `Idle iff no resync is running`.

### Shared scenarios

**State × action for one tab in this GUI.** Each cell: what the user sees / client state change /
messages sent / race behaviour / exerciser.

| Tab state \ action | Window resize step | Resync, same topology | Resync, different topology | Zoom toggled in this GUI | Zoom changed by the server |
|---|---|---|---|---|---|
| In sync | Tab takes the window size / local split data adjusted / one Resize per pane whose size changed (C2) / a later stale report cannot reach the server (C3) / harness `burst` | Nothing visible / active and zoom updated / none / — / unit test `resync_keeps_local_sizes` | Server's tree shown / tree and sizes replaced, `requested_size` set per pane / none / sizes now the server's until the next window resize / unit test `resync_adopts_new_topology_without_resize` | Pane fills the tab / zoom set, zoomed pane resized to tab size / SetPaneZoomed and Resize for that pane / — / harness `zoom` | Pane fills or unfills / zoom taken, local tab size re-applied / Resize only for panes whose size changed (C2) / — / unit test `resync_zoom_change_reapplies_local_size` |
| Requests in flight (server behind) | Same as in sync; newer requests queue behind older ones | No sizes taken; the in-flight requests land and settle the server / none / the old loop cannot start (R1) / harness `burst`, counter `send.Resize.in_resync` = 0 | Same as in sync | Same as in sync | Same as in sync |
| Zoomed | Zoomed pane takes the window size / other panes' split data adjusted / Resize for the zoomed pane / the server's stale tab size is never taken back (C3) / harness `zoom` | Zoomed pane keeps the window size | Server's tree and sizes taken; the zoomed pane may get a stale size until the next window resize | Unzoom: panes take their split sizes / Resize per pane that changed | — |
| New to this GUI (attach, spawn from cli) | As in sync | Not reachable: there is no local tree, so the topology is new by construction | Tab created with the server's sizes, `requested_size` from each entry / none sent / the `TabAddedToWindow` rule (grow the window to fit) is unchanged / harness `attach` | — | — |

Invariants:
- `requested_size == Some(s)` iff the last size this pane sent, adopted or was created with
  is `s`. Set in the same call that sends or adopts.
- `ResyncState::Idle` iff no resync future is running.

Omitted states, with reasons: "server unreachable": resync fails, as today, with no change
here. "Pane closed during a resync": the topology differs and the server's tree is taken, as
today.

## Implementation Units

### U1. Count the Resize messages sent from inside a resync

- **Goal:** make R1 measurable: a counter that reads above 0 on today's engine and 0 after U3.
- **Requirements:** R1
- **Dependencies:** none. Lands on `unit/client-resync-counters`, which already counts
  `mux.client.send.Resize`.
- **Files:** Modify `wezterm-client/src/domain.rs`, `wezterm-client/src/pane/clientpane.rs`.
- **Approach:** a flag scoped to `process_pane_list`, read where `ClientPane::resize` sends,
  feeding a `mux.client.send.Resize.in_resync` counter. This is the probe's mechanism, kept as a
  metric.
- **Test scenarios:** *Integration:* harness `burst` on the engine without U3 → counter > 0.
- **Verification:** the stats table shows the counter, and it is above 0 on today's engine.
- **Checkpoint:** auto — harness `burst` against the current engine; the counter must be > 0
  (proves it can say yes).

### U2. Keep the resize harness in the fork

- **Goal:** turn the throwaway harness into the durable exerciser for R1–R4.
- **Requirements:** R1, R2, R3, R4, R5
- **Dependencies:** none (fork-only files on `build`).
- **Files:** Create `fork/resize-harness/probe.lua`, `fork/resize-harness/run.zsh`,
  `fork/resize-harness/check.py`, `fork/resize-harness/README.md`.
- **Approach:** the scripts that ran on 2026-10-02 under `/private/tmp/claude-501/resizeab`. The
  checks are window-size match per tab, split proportions within one cell of the starting ratio,
  zoomed-pane size, and the U1 counter and `mux.client.resync` read from the stats table. The
  scenarios are `burst`, `slow`, `zoom`, `attach` and `cli-split` (R5). The server binary is a
  parameter; the default is the 2026-09-20 engine copy.
- **Test scenarios:** *Happy path:* `slow` on today's engine → all even. *Error path:* `zoom` on
  today's engine → 4 of 4 zoomed panes off-size (red).
- **Verification:** each scenario prints pass or fail per check and exits non-zero on failure.
- **Checkpoint:** auto — `zoom` and `burst` fail on today's engine and `slow` passes.

### U3. The client sends only sizes it chose (`unit/resync-no-resize-echo`)

- **Goal:** C1, C2 and C3: no echo, splits and zoom keep the GUI's sizes, against the old server.
- **Requirements:** R1, R2, R3, R5
- **Dependencies:** `unit/tabresized-on-change`, the base branch, whose announcement rule C3
  keeps and whose code in `sync_with_pane_tree` this unit changes.
- **Files:**
  - Modify `mux/src/tab.rs` (`sync_with_pane_tree`, new `is_same_topology`) and
    `wezterm-client/src/pane/clientpane.rs` (`requested_size`, `resize`, `adopt_server_size`,
    `new`).
  - Modify `wezterm-client/src/domain.rs` (`process_pane_list` adopts sizes for changed
    topologies).
  - Test `mux/src/tab.rs` (test module; `FakePane` gains `get_dimensions` and a resize counter).
- **Approach:**
  - `sync_with_pane_tree` returns whether the topology changed and stops calling
    `apply_size` on the server's tree.
  - For the same topology it writes only active and zoom, mirroring the existing direct
    writes at `mux/src/tab.rs:793-834`, and re-applies the local size only when zoom changed.
  - The `requested_size` memory mirrors `focused_remote_pane_id` (`wezterm-client/src/domain.rs:28`).
- **Test scenarios:**
  - *Happy path:* `resync_keeps_local_sizes`: local tree 40|39, server tree of the same
    topology with 49|49 → local sizes unchanged, `FakePane` resize count 0.
  - *Edge:* `resync_adopts_new_topology_without_resize`: server adds a pane → tree replaced,
    resize count 0, `is_same_topology` false.
  - *Edge:* `resync_zoom_change_reapplies_local_size`: server zooms pane B → B resized to the
    local tab size once.
  - *Edge:* `is_same_topology` false for the same panes split in the other direction, and for a
    swapped pane order.
  - *Integration:* harness `burst`, `zoom`, `attach`, `cli-split` against the 2026-09-20 server.
- **Verification:** U1 counter 0 under `burst`. Every tab at the window size after the burst.
  Zoomed panes at the window size. Splits within one cell of their starting proportions. A
  `wezterm cli split-pane` from outside appears in the GUI.
- **Checkpoint:** auto — `cargo test -p mux --lib tab::` (each new test goes red with its defect
  planted: an `apply_size` call restored in the resync, or `is_same_topology` returning true),
  then harness `burst` and `zoom` green.

### U4. Coalesce resyncs (`unit/resync-coalesce`)

- **Goal:** C4: a burst of server `TabResized` messages costs one running resync and at most one
  queued, instead of one per message, each re-reading every tab on the main thread.
- **Requirements:** R4
- **Dependencies:** none (base `upstream/main`).
- **Files:** Modify `wezterm-client/src/client.rs` (the `Pdu::TabResized | Pdu::TabAddedToWindow`
  arm) and the resync completion in `wezterm-client/src/domain.rs`. Test: the `ResyncState`
  transitions, as a unit test in `wezterm-client`.
- **Approach:** a `ResyncState` on the client connection, under the same lock discipline as the
  existing `ClientInner` fields.
- **Test scenarios:**
  - *Happy path:* Idle + TabResized → Running, one resync.
  - *Edge:* Running + 40 TabResized → RunningAndQueued; on completion, exactly one more
    resync, then Idle.
  - *Error path:* a resync that fails returns to Idle and still honours a queued request.
- **Verification:** `mux.client.resync` per `burst` ≤ 2 × window resize steps (against 231 for
  4 resizes before).
- **Checkpoint:** auto — the `ResyncState` unit test (red if the queued flag is dropped), then
  the harness `burst` counter.

## Scope Boundaries

- No codec change and no server-side change in this plan.
- The cost of each tab bar rebuild (`format-tab-title` converting the whole config) is
  `unit/pr-8131-tabbar-lua-once` and the open config-once fix, not this plan.
- Tab titles echoing between client and server (wezterm/wezterm#7749) are not covered.
- Re-centering splits that have already drifted is not covered.

### Deferred to Follow-Up Work

- Server-side zoom: `rebuild_splits_sizes_from_contained_panes` should update the tab size when
  a zoomed pane is resized, so `root_size()` stops reporting the pre-zoom size to new clients
  and `wezterm cli list`. It takes effect only on a mux restart. Recorded as a `BACKLOG.md`
  lane.
- A one-time re-centering of already-drifted splits (`wezterm cli adjust-pane-size`), offered
  to the owner separately.
- The `TabAddedToWindow` rule that grows the window to the larger of window and tab can take a
  stale server size at attach. Watch it at the live probe; it is upstream behaviour kept as is.

## System-Wide Impact

- **Interaction graph:** `apply_dimensions` → `Tab::resize` → `ClientPane::resize` (C2);
  server `Pdu::TabResized` → `ResyncState` (C4) → `process_pane_list` →
  `sync_with_pane_tree` (C3) → `adopt_server_size` (C2). The GUI's `TabResized` handler and the
  tab bar are unchanged.
- **Unchanged invariants:**
  - The server code and the wire format.
  - `RenderableState.dimensions` still follows server render deltas, so rendering is unaffected.
  - Split drag, `AdjustPaneSize` and zoom made in this GUI still send.
  - The `TabAddedToWindow` window-growing rule.
  - The race between a local split and the resync that reports it is unchanged.
- **Integration coverage:** only the harness against a real server exercises C1–C4 together;
  unit tests cover C3's predicate and the `ResyncState` machine.

## Disconfirming Evidence

- **Probe gate, kill condition:** after U3, harness `burst` (20 split tabs, 20 steps 40 ms apart,
  2026-09-20 server) must end with every tab at the window size and the U1 counter at 0. A tab
  off the window size with the counter at 0 means another writer, most likely the
  `TabAddedToWindow` rule or render deltas feeding `get_size`, and the design needs a GUI re-fit
  step before U4.
- **Live gate:** after deploy, one relaunch and one fullscreen toggle on `human-main`: the
  counters show 0 in-resync sends, and the split check over `wezterm cli list` shows no new skew
  against a snapshot taken before the toggle.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| A caller of `ClientPane::resize` the lanes missed still sends server sizes | The U1 counter catches any send during a resync; the harness `burst` gates on it |
| Resize messages reach the server out of order in a release build (unverified; the trace was a debug build) | Without echoes the last request wins only if order holds. If the harness shows a final size other than the last request, the rejected generation approach becomes the fix, and with it a planned mux restart |
| Keeping local sizes hides a legitimate size change by another client | One GUI attaches here. Recorded as a trade-off; R5 covers structure changes, which still flow |
| `unit/tabresized-on-change` and U3 touch the same function, and `build` is reassembled by more than one session | U3 is based on that unit, so the merge order is fixed and no merge-commit resolution is needed |
