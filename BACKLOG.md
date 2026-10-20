# Backlog

Units that have no branch yet. `PATCHES.md` holds every unit that has one; an
entry leaves this file at the moment `git switch -c unit/<slug>` is run. The
entries the plan of 2026-09-06 proposed keep its relative order, minus
`mux-socket-buffers`, which has a branch and so lives in `PATCHES.md`.
That plan's `tab-order-over-protocol` has a branch now and lives in
`PATCHES.md`. `flash-message`, `picker-modal` and
`mux-push-queue-bound` came later and have no place in that order yet. Deferred units are decided against
for the current topology, not retired; they return if the need does. The
`notes:` lines name investigation notes in `.inbox/`, which is local to this
machine and not part of this repo.

## Open

`pr-7763-focus-storm` was absorbed by `unit/pr-7871-focus-storm` (carried #7871, not #7763 `44a8f93`).

### `tab-bar-title-memo`

Named `tabbar-title-memo` until 2026-10-02; plans written before then use that name.

- side: gui
- state: proposed
- needs: a measurement of the rebuild cost left once `title-update-coalesce` runs, to show whether memoizing is still worth its complexity
- why: Marshal-once is `pr-8131-tabbar-lua-once`; one config conversion per rebuild is `tab-bar-config-once`; one rebuild per burst of title notifications is `title-update-coalesce`, which also covers the OSC 0 double alert. Remaining: memoize each tab title on its inputs, so a title change in one pane does not run `format-tab-title` for every tab, and convert the config once per config generation rather than once per rebuild. On 2026-10-17 an engine with the first two still spent 26 to 28 ms per rebuild of 41 tabs, so the remaining benefit is real but unmeasured until the coalesce unit has run
- notes: `2026-09-07-tab-title-rebuild-config-conversion-cost.md`, `2026-09-07-osc0-double-alert-no-title-coalescing.md`

### `server-zoomed-tab-size`

- side: server
- state: proposed
- needs: a mux restart to take effect, which ends every session; plan `docs/plans/2026-10-02-001-fix-client-size-authority-plan.md` covers the client side without one
- why: `rebuild_splits_sizes_from_contained_panes` returns early for a zoomed tab, so after a zoomed pane is resized the server's tab size and split data stay at their pre-zoom values and `PaneNode::root_size()` keeps reporting them. A GUI that attaches takes that stale size, and `wezterm cli list` shows it. Reproduced 2026-10-02: 4 of 4 zoomed panes stayed at their zoom-time 120x30 in a throwaway harness, with and without `unit/tabresized-on-change`; live tabs held zoomed panes at 156x44, 173x44 and 80x24 in a 173x43 window
- notes: none; the harness is `fork/resize-harness/` once U2 of that plan lands

### `split-pixel-size-on-zero-delta`

- side: gui
- state: proposed
- needs: a decision on whether a resize that changes the pixels per cell, and leaves a split's cell counts as they were, should refresh the pixel and dpi sizes below that split
- why: `adjust_x_size` and `adjust_y_size` return at once for a zero cell delta, so a split whose cell count does not change keeps the pixel sizes and dpi it had, though the window's pixels per cell changed. Probe 2026-10-04: a split 5 columns wide (two panes of 2) beside a 94-column pane, resized from 100 to 102 columns at 20 pixels per column instead of 10. On `resync-no-resize-echo` one of the two small panes kept 20 pixels where 40 was expected; with `split-resize-keeps-ratio` both do, because a small split's share now often rounds to the same cell count while the window grows. Found by a code review of that unit
- notes: none

### `cli-adjust-pane-size-not-shown`

- side: gui
- state: proposed
- needs: a harness scenario that runs `wezterm cli adjust-pane-size` against the server and reads the GUI's `list`, to show whether it is real
- why: by reading the code, 2026-10-04, not run: `Pdu::AdjustPaneSize` changes the server's tab (`wezterm-mux-server-impl/src/sessionhandler.rs`), the server announces `TabResized`, and the GUI re-reads the tree. Since `resync-no-resize-echo`, `sync_with_pane_tree` keeps the GUI's own sizes when the same panes are split the same way, so the change would not show, and the GUI's next resize would send its sizes back over it. Upstream replaces the GUI's tree with the server's on every re-read (`upstream/main`, `Tab::sync_with_pane_tree`), so there it shows. The plan's requirement R5 lists a split, spawn, close and zoom made outside the GUI, not a size change
- notes: none

### `uservars-replay-on-attach`

- side: protocol
- state: proposed
- needs: GUI and mux server built from the same commit
- why: A `ClientPane` starts with an empty `user_vars` map and learns only from live `Alert::SetUserVar`, so state carried in user vars is missing after every GUI relaunch. Send each pane's current vars when a client attaches it
- notes: `2026-09-06-viterm-freeze-fork-plan.md`

### `vertical-tabs`

- side: gui
- state: proposed
- needs: A choice between upstream PRs #7574 (17 files) and #7679 (10 files). The plan expects a conflict with `tabbar-title-memo` in `tabbar.rs`
- why: A vertical tab bar, carried from upstream
- notes: `2026-09-06-viterm-freeze-fork-plan.md`

### `flash-message`

- side: gui
- state: specified
- needs: a person to look at it once built, because all four checks in its spec are visual. The 0.96→1 scale is not a prerequisite: the spec allows dropping it if it cannot be done without re-shaping text per frame
- why: `window:flash(text)`: large centred numerals over the panes for transient feedback, so the tab-jump digits do not have to live in the status bar. Chosen from four rendered candidates
- notes: `2026-09-18-flash-message-unit-spec.md`

### `picker-modal`

- side: gui
- state: proposed
- needs: A choice between extending `InputSelector` in place, where it stays a grid of cells drawn over the pane (`overlay/selector.rs`), and a new picker built as a `Modal` on `box_model`, as the command palette is (`termwindow/palette.rs`). The first is smaller; the second gets padding, borders and mixed font sizes. Either way the Lua surface is the part that could go upstream, so it is decided first
- why: `InputSelector` is how a config shows a list of its own rows, and Rust fixes its keys: Enter or a pick letter calls the action once with the chosen row's id and label, Escape calls it with none, and the picker closes (`overlay/selector.rs:207`, `:308`, `:370`). It takes its rows when it opens and reads nothing but keys and mouse after that. So one list cannot offer two actions (copy on Enter, open on another key), one row cannot be dismissed alone, Escape does not say which row was highlighted, a heading cannot be unselectable, and nothing can describe the highlighted row. In order of size, a picker could gain: a key map in its config, with the key's name passed to the action; section headings and a detail line for the highlighted row; an action that keeps the list open and returns new rows; rows that change while it is open, which needs a path from the main thread back into the picker that does not exist today. `box_model` draws text, nested boxes and polygons only (`ElementContent`, `box_model.rs:394`), so an image preview in a picker is renderer work outside this unit. Read at `9000b7ec0`; the fork has not changed `selector.rs`, `modal.rs` or the `InputSelector` struct
- notes: `2026-10-03-picker-modal-consumers-and-source.md`

### `mux-push-queue-bound`

- side: server
- state: proposed
- needs: a sign that the queue grows in practice: the GUI's `get_pane_render_changes` count stops rising while the mux server's footprint climbs. Until that is seen, a change to the push path has nothing to prove itself against
- why: Each session's outgoing queue (`dispatch.rs`, `smol::channel::unbounded::<Item>()`) takes every push with `try_send`, and `compute_changes()` queues a full `GetPaneRenderChangesResponse` for each output event of each subscribed pane, so one connected client that stops reading grows the server without limit. `mux-render-subscribers` stopped pushes to connections that subscribe to nothing, the multiplier inferred for the server that grew to 296 GB and was killed on 2026-10-08 (queue sizes were not recorded). Since then one server ran from 2026-10-10 to 2026-10-19 between 330 MB and 1.85 GB, and grew 180 MB during seven hours of a saturated GUI main thread on 2026-10-17. If it is built: keep at most one pending render push per pane per session, the newest replacing an unsent one, so a stalled reader costs one push per pane; closing a session whose queue passes a byte limit is the alternative
- notes: `2026-10-11-per-session-push-queue-still-unbounded.md`

### `tab-bar-drag`

- side: gui
- state: proposed
- needs: `unit/tab-groups` (base). Plan `docs/plans/2026-10-20-001-feat-tab-groups-plan.md` U10.
- why: A press, drag and release on a tab or a folded tab group in the fancy tab bar should move it, a group moving its whole run, through `Window::set_tab_order` and `WindowTabOrderChanged` as `move_tab` does; a config that keeps a folder order (bootstrap's `tab_groups.lua`) then reads the new order on its next tick. Today the order changes only through `MoveTab` keys and Lua. The new order is a pure function of (items, dragged item, drop slot), which is the tested seam; no tool here drives mouse input into a native window, so the press path waits for a person.

### `tab-bar-motion`

- side: gui
- state: proposed
- needs: `unit/tab-groups` (base). Plan `docs/plans/2026-10-20-001-feat-tab-groups-plan.md` U11.
- why: When a folded group opens or closes, or a tab moves into its group, the drawn boxes jump between layouts. Keeping each item's previous rect by key and easing to the new one over about half a second, with frames scheduled through `update_next_frame_time` (the spinner's path), would show the move without any Lua during it; with the window unfocused the final layout is drawn at once. `colorease.rs` has curves; nothing eases a position yet.

## Deferred

Decided against for the current topology (one GUI on `human-main`). Not retired; the row returns to Open if the need does.

### `viewer-selection-isolation`

- side: protocol
- state: deferred (2026-09-20)
- needs: A second viewer attached on purpose. Until then this machine wants shared selection: `wezterm cli activate-pane-direction` is supposed to move the one GUI. A codec bump and mux restart for a two-GUI design would be the wrong cost
- why: Focusing a pane in one GUI moves every other attached GUI's active tab, and a GUI receives its own focus echo. Reproduced 2026-09-19 on a throwaway mux: raising one window moved the other from TAB-0 to TAB-2; the trigger is OS window focus, not a headless activation. The bug is real and not this machine's. There is one GUI; a second on `human-main` is forbidden because it acks attention markers, and isolation would not lift that. The freeze is `pr-7763`, not this. Tab order on relaunch is the unit `tab-order-over-protocol`, which does not depend on this. Build it if a second viewer is ever a workflow
- notes: `2026-09-19-mux-viewer-selection-and-tab-order.md`

### `termwiz-repaint-follow-ups`

- side: library
- state: deferred (2026-10-19)
- needs: a program on this machine that paints through termwiz's `BufferedTerminal`, or an upstream reviewer of `blank-row-keeps-reset` asking about one of these. Nothing in the engine does today: outside `termwiz/` and `wezterm-surface/`, no code uses `BufferedTerminal` or `Surface::get_changes` (searched at `df473bbd6`)
- why: Two faults in `Surface::repaint_all()`, found by reading while `blank-row-keeps-reset` was reviewed; neither was run. (1) The backward scan for a trailing run of clears takes a line that has text and ends in `ClearToEndOfLine(color)` as part of the run, so on a screen cleared to a colour the forward pass emits `ClearToEndOfScreen` in place of such lines: `Surface::new(3, 2)`, `ClearScreen(Red)`, `"a"`, a move to row 1, `"b"` would leave `"b"` unpainted. (2) `repaint_all()` does not end with the surface's current attributes, so a delta drawn after a repaint starts from whatever the last line left.
- notes: `2026-10-19-get-text-escapes-drops-reset-after-blank-row.md`
