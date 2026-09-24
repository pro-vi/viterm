# Backlog

Units that have no branch yet. `PATCHES.md` holds every unit that has one; an
entry leaves this file at the moment `git switch -c unit/<slug>` is run. The
entries the plan of 2026-09-06 proposed keep its relative order, minus
`mux-socket-buffers`, which has a branch and so lives in `PATCHES.md`.
`tab-order-on-reattach` replaces that plan's `tab-order-over-protocol` (the
need, not a chosen mechanism). `flash-message` came later and has no place
in that order yet. Deferred units are decided against
for the current topology, not retired; they return if the need does. The
`notes:` lines name investigation notes in `.inbox/`, which is local to this
machine and not part of this repo.

## Open

`pr-7763-focus-storm` was absorbed by `unit/pr-7871-focus-storm` (carried #7871, not #7763 `44a8f93`).

### `tabbar-title-memo`

- side: gui
- state: proposed
- needs: Open question: whether the OSC 0 double alert is fixed inside this unit or as a unit of its own
- why: Marshal-once is `pr-8131-tabbar-lua-once`. Remaining: memoize each tab title on its inputs, coalesce title updates in one event-loop turn, and the OSC 0 double-alert (`2026-09-07-osc0-double-alert-no-title-coalescing.md`). With a `format-tab-title` handler registered, one title notification still rebuilds every tab and still converts the whole config into a Lua table. The size of the remaining benefit is unmeasured until it is built
- notes: `2026-09-07-tab-title-rebuild-config-conversion-cost.md`, `2026-09-07-osc0-double-alert-no-title-coalescing.md`

### `uservars-replay-on-attach`

- side: protocol
- state: proposed
- needs: GUI and mux server built from the same commit
- why: A `ClientPane` starts with an empty `user_vars` map and learns only from live `Alert::SetUserVar`, so state carried in user vars is missing after every GUI relaunch. Send each pane's current vars when a client attaches it
- notes: `2026-09-06-viterm-freeze-fork-plan.md`

### `tab-order-on-reattach`

- side: gui or protocol, depending on the mechanism
- state: proposed
- needs: A choice of mechanism. Isolation is not a prerequisite while there is one GUI. The protocol path needs GUI and mux server built from the same commit. The config-replay path needs no codec change and no mux restart
- why: A reordered tab reverts on GUI relaunch / engine install, because `codec` has no tab-move message. The server does hold an order; nothing can permute it. Reproduced 2026-09-19: `MoveTabRelative` in one GUI was invisible to the server and gone after reattach. That is the useful part for this machine: ~28 tabs as spatial memory. Two candidate solutions, neither chosen: (1) canonical `MoveTab` over the protocol, the original `tab-order-over-protocol` unit — an existing client is not re-ordered by a resync, so the order needs its own apply path, and a local window can span several remote windows, so a move names a remote window and a remote tab; (2) Lua replay on attach from `window:mux_window():tabs()` plus `MoveTab`, no protocol change. (2) has to key on tab title or a pane's launch id, because a Lua `tab:tab_id()` is a local id regenerated per GUI process, so it matches by name rather than identity. Drawn-bar jump already uses `mux_window:tabs_with_info()` and does not need this
- notes: `2026-09-19-mux-viewer-selection-and-tab-order.md`, `2026-09-06-viterm-freeze-fork-plan.md`

### `vertical-tabs`

- side: gui
- state: proposed
- needs: A choice between upstream PRs #7574 (17 files) and #7679 (10 files). The plan expects a conflict with `tabbar-title-memo` in `tabbar.rs`
- why: A vertical tab bar, carried from upstream
- notes: `2026-09-06-viterm-freeze-fork-plan.md`

### `flash-message`

- side: gui
- state: specified
- needs: Open question: whether the 0.96→1 scale can be done without re-shaping text per frame
- why: `window:flash(text)`: large centred numerals over the panes for transient feedback, so the tab-jump digits do not have to live in the status bar. Chosen from four rendered candidates
- notes: `2026-09-18-flash-message-unit-spec.md`

## Deferred

Decided against for the current topology (one GUI on `human-main`). Not retired; the row returns to Open if the need does.

### `viewer-selection-isolation`

- side: protocol
- state: deferred (2026-09-20)
- needs: A second viewer attached on purpose. Until then this machine wants shared selection: `wezterm cli activate-pane-direction` is supposed to move the one GUI. A codec bump and mux restart for a two-GUI design would be the wrong cost
- why: Focusing a pane in one GUI moves every other attached GUI's active tab, and a GUI receives its own focus echo. Reproduced 2026-09-19 on a throwaway mux: raising one window moved the other from TAB-0 to TAB-2; the trigger is OS window focus, not a headless activation. The bug is real and not this machine's. There is one GUI; a second on `human-main` is forbidden because it acks attention markers, and isolation would not lift that. The freeze is `pr-7763`, not this. Tab order on relaunch is `tab-order-on-reattach` and does not depend on this while there is one GUI. Build it if a second viewer is ever a workflow
- notes: `2026-09-19-mux-viewer-selection-and-tab-order.md`
