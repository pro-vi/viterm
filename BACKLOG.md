# Backlog

Units that have no branch yet. `PATCHES.md` holds every unit that has one; an
entry leaves this file at the moment `git switch -c unit/<slug>` is run. The
entries the plan of 2026-09-06 proposed keep its relative order, minus
`mux-socket-buffers`, which has a branch and so lives in `PATCHES.md`.
`viewer-selection-isolation`, `flash-message` and `mux-tab-get-index` came
later and have no place in that order yet. The `notes:` lines name
investigation notes in `.inbox/`, which is local to this machine and not part
of this repo.

## Open

### `pr-7763-focus-storm`

- side: both
- state: proposed
- needs: The GUI half works against the running mux server; the server half goes live only when the mux server is restarted
- why: The GUI burns a core and about 1.3 GB an hour for hours after a pane is added to a mux tab. Carried from upstream PR #7763 (fculpo `44a8f93`) without its `Cargo.toml` hunk, applied with `git am`
- notes: `2026-09-06-viterm-freeze-fork-plan.md`, `2026-09-07-focus-loop-ends-when-panes-split-apart.md`

### `tabbar-title-memo`

- side: gui
- state: proposed
- needs: Open question: whether the OSC 0 double alert is fixed inside this unit or as a unit of its own
- why: With a `format-tab-title` handler registered, one title notification rebuilds every tab's title and each rebuild converts the whole config into a Lua table. Memoize each tab's title on its inputs and coalesce title updates within one event-loop turn. The size of the benefit is unmeasured until it is built
- notes: `2026-09-07-tab-title-rebuild-config-conversion-cost.md`, `2026-09-07-osc0-double-alert-no-title-coalescing.md`

### `client-move-pane-id`

- side: gui
- state: proposed
- why: `wezterm-client/src/domain.rs` `split_pane` translates `--pane-id` and forwards `--move-pane-id` untranslated, so moving a pane on a mux window moves the wrong one
- notes: `2026-09-06-viterm-freeze-fork-plan.md`

### `uservars-replay-on-attach`

- side: protocol
- state: proposed
- needs: GUI and mux server built from the same commit
- why: A `ClientPane` starts with an empty `user_vars` map and learns only from live `Alert::SetUserVar`, so state carried in user vars is missing after every GUI relaunch. Send each pane's current vars when a client attaches it
- notes: `2026-09-06-viterm-freeze-fork-plan.md`

### `viewer-selection-isolation`

- side: protocol
- state: proposed
- needs: GUI and mux server built from the same commit; the GUI's version check requires equal codec versions, so every GUI and the mux server move together. Ships before `tab-order-over-protocol`. Owed first and unrun: the two-GUI reproduction, on a throwaway mux server rather than `human-main`, since a second GUI on the live one clears attention markers before the human sees them
- why: Focusing a pane in one GUI moves every other attached GUI's active tab, and a GUI receives its own focus echo. `advise_focus` sends `SetFocusedPane`, the server writes the shared `active_tab_idx` and broadcasts `PaneFocused` without an origin filter, and every client applies it. Suppressing the broadcast is not sufficient: `sync_with_pane_tree` resets the active pane from the server's `is_active_pane` on every pane-list refresh, for tabs the client already has. A viewer reports where it is; only an addressed command changes where another viewer is
- notes: `2026-09-19-mux-viewer-selection-and-tab-order.md`

### `tab-order-over-protocol`

- side: protocol
- state: proposed
- needs: GUI and mux server built from the same commit. Best built after `viewer-selection-isolation`, on the invariant that a reorder never changes any viewer's selection
- why: A reordered tab reverts on reattach, because `codec` has no tab-move message. The server does hold an order; nothing can permute it. Two constraints found since this entry was written: an existing client is not re-ordered by a resync, since the pane-list loop only pushes a tab it does not already have, so the order needs its own apply path; and a local window can hold tabs from several remote windows, so a move names a remote window and a remote tab, and a move across two such groups is not a canonical reorder at all
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

### `mux-tab-get-index`

- side: gui
- state: proposed
- why: `MuxTab` has no way to report where it sits, while `window:tabs_with_info()` has `index` and `format-tab-title` gets `tab_index`. Everyone reaches for `tab:get_index()` because every neighbouring API has it, and a config that does gets `attempt to call a nil value` at runtime with the reason only in a log file. Cost two real bugs in bootstrap's config on 2026-09-18. About twenty lines and a docs page, and the first thing here worth sending upstream as a PR
