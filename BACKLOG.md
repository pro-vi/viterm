# Backlog

Units that have no branch yet. `PATCHES.md` holds every unit that has one; an
entry leaves this file at the moment `git switch -c unit/<slug>` is run. The
first six are in the order the plan of 2026-09-06 picks them up; the last two
came later and have no place in that order yet. The `notes:` lines name
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

### `tab-order-over-protocol`

- side: protocol
- state: proposed
- needs: GUI and mux server built from the same commit. Marked optional and the largest in the plan
- why: A reordered tab reverts on reattach, because `codec` has no tab-move message
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
- needs: Open question: whether the 0.96→1 scale can be done without re-shaping text per frame
- why: `window:flash(text)`: large centred numerals over the panes for transient feedback, so the tab-jump digits do not have to live in the status bar. Chosen from four rendered candidates
- notes: `2026-09-18-flash-message-unit-spec.md`

### `mux-tab-get-index`

- side: gui
- state: proposed
- why: `MuxTab` has no way to report where it sits, while `window:tabs_with_info()` has `index` and `format-tab-title` gets `tab_index`. Everyone reaches for `tab:get_index()` because every neighbouring API has it, and a config that does gets `attempt to call a nil value` at runtime with the reason only in a log file. Cost two real bugs in bootstrap's config on 2026-09-18. About twenty lines and a docs page, and the first thing here worth sending upstream as a PR
