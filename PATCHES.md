# Patches carried on `build`

`main` mirrors `upstream/main` and carries nothing. Each change lives on its own
`unit/<slug>` branch, based on `upstream/main`, containing only that change and
written so it could be sent upstream as it stands. `build` is `upstream/main`
plus a merge of every active unit, and every local build comes from `build`.
This file, `BACKLOG.md`, `CLAUDE.md` and `docs/plans/` exist only on `build`;
`CLAUDE.md` is where the rules live, this file is what they produced. The
`.inbox/` investigation notes stay local to this machine and are not tracked.

A row here means a branch exists: `git branch --list 'unit/*'` and this table
must name the same units. Work that has no branch yet is in `BACKLOG.md`, and its
entry moves here at the moment its branch is created. `base` is the branch the
unit was cut from and is rebased onto: `upstream/main`, or the unit it cannot
compile without. A merge conflict with another already-merged unit is noted on
the incoming row; the resolution lives in the `--no-ff` merge commit, not on
the unit branch. Absorbing, joining, and dropping units is in `CLAUDE.md`.

A carried third-party patch is applied with `git am` so authorship stays with
its author, and its row says `carried`. `git cherry upstream/main unit/<slug>`
reports when a unit has landed upstream verbatim; a rework upstream needs this
file edited by hand.

| slug | branch | base | side | status | why | files | proving test |
|---|---|---|---|---|---|---|---|
| `tab-bar-rows` | `unit/tab-bar-rows` | `upstream/main` | gui | local, ours | The fancy tab bar was one row, so with 26 tabs open each title was cut to five or six characters and the bar could only be navigated by position. `tab_bar_rows` makes the bar N rows tall and spreads the tabs over the rows, dividing the width budget by the tabs on one row rather than by the total. Default 1 leaves the existing element tree untouched. | `config/src/config.rs`, `wezterm-gui/src/termwindow/render/tab_bar.rs`, `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs`, `docs/config/lua/config/tab_bar_rows.md` | With 26 tabs and `tab_bar_rows = 2`: the bar draws two rows, tabs 1–14 on the first beside the status areas and 15–26 plus the new-tab button on the second, and the tab hit regions sit in two bands (measured at dpi 144: row 1 y=692 h=48 holding tab_idx 0–13, row 2 y=742 h=48 holding 14–25), so a click on the second row activates the tab under it. Tab widths came out 137–215 px against a one-row cap near 108. A run with `tab_bar_rows = 1` must be pixel-identical to upstream. |
| `tab-bar-row-status` | `unit/tab-bar-row-status` | `unit/tab-bar-rows` | gui | local, ours | With more than one row, only the first could show a status area, so the rows below held tabs and nothing else while the status competed with the tabs on row one. `window:set_left_status()` and `set_right_status()` take an optional row number counting from 1; the window keeps one status string per row and `TabBarItem::LeftStatus`/`RightStatus` carry their row. Omitting the number is what every existing config does and means row one. | `wezterm-gui/src/tabbar.rs`, `wezterm-gui/src/termwindow/mod.rs`, `wezterm-gui/src/termwindow/mouseevent.rs`, `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs`, `wezterm-gui/src/scripting/guiwin.rs`, `docs/config/lua/window/set_{left,right}_status.md` | With `tab_bar_rows = 2` and a status written to row 2, both rows show their own left and right runs and the first row's git cells are unchanged; with no row argument anywhere, the bar is identical to before. |
| `tab-bar-rows-fill` | `unit/tab-bar-rows-fill` | `unit/tab-bar-row-status` | gui | local, ours | With `tab_bar_rows = 2` the tabs were split evenly, so 17 tabs became 9 and 8 while the first row had room for all of them. Each row now takes tabs in order while their laid-out widths (measured with the same layout context as the bar) fit beside that row's status areas; the last row takes the rest, and a row always takes at least one tab. The per-tab width cap is unchanged and sized so the configured rows hold every tab, so filling never needs more rows than the even split. Merged into `build` without conflict. | `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` | Throwaway GUI, no plugins, `tab_bar_rows = 2`, 17 titled tabs: at 230 columns all 17 sit on row 1 (was 9 and 8); at 130 columns row 1 fills to the window edge with 11 and row 2 holds 6 plus the new-tab button. Titles cut at 130 columns come from the unchanged width cap, not the fill. |
| `pr-7871-focus-storm` | `unit/pr-7871-focus-storm` | `upstream/main` | both | carried | GUI pins a core after a pane is added to a mux tab (`SetFocusedPane`/`PaneFocused` echo). Carried from wezterm/wezterm#7871 (ankitson; successor of #7763), minus the Cargo.toml chrono `clock` hunk already in tree. GUI half is live against the running mux; server half waits for mux restart. Merges without conflict since the tab-bar units were rebased onto the `upstream/main` that carries #8018; the alignment change from #8018 now lives in `unit/tab-bar-rows`. | `mux/src/tab.rs`, `mux/src/lib.rs`, `mux/src/tmux_commands.rs`, `wezterm-client/src/pane/clientpane.rs`, `wezterm-gui/src/frontend.rs`, `wezterm-mux-server-impl/src/sessionhandler.rs` | With the one GUI on `human-main`, a two-pane mux tab (split or wait): GUI under 10% of a core for an hour and `wezterm cli list` snapshots show a stable `is_active` per tab (`~/.local/state/wezterm-storm/`). |
| `pr-8131-tabbar-lua-once` | `unit/pr-8131-tabbar-lua-once` | `upstream/main` | gui | carried | `format-tab-title` marshalled every tab and pane on every call, twice per tab per rebuild. Carried from wezterm/wezterm#8131 (iliaal; option 2 of #8086). Not title memo, not coalesce, not OSC 0. Conflicts with `unit/tab-bar-row-status` in `wezterm-gui/src/tabbar.rs`; resolution is in the `--no-ff` merge commit, not on this unit branch. | `wezterm-gui/src/tabbar.rs` | Proven with the 7871 soak on the live window. Does not claim the config-to-Lua conversion cost is gone. |
| `mux-tab-get-index` | `unit/mux-tab-get-index` | `upstream/main` | gui | local, ours | `MuxTab` has `tab_id` and `window` but no way to report where the tab sits, while `window:tabs_with_info()` has `index` and `format-tab-title` gets `tab_index`. `tab:get_index()` returns that 0-based index, or nil if the tab is not in a window. | `lua-api-crates/mux/src/tab.rs`, `docs/config/lua/MuxTab/get_index.md`, `docs/changelog.md` | Throwaway GUI, empty `unix_domains`: before, `pcall(tab.get_index, tab)` is `attempt to call a nil value` and `tabs_with_info()[1].index` is 0. After, `tab:get_index()` returns 0. |
| `client-move-pane-id` | `unit/client-move-pane-id` | `upstream/main` | gui | local, ours | `ClientDomain::split_pane` translates `--pane-id` to the remote id and forwarded `--move-pane-id` untranslated, so a move against a mux window looked up the GUI-local id on the server. The unit translates the moved pane the same way as the target and reparents the existing local ClientPane instead of wrapping it twice. | `wezterm-client/src/domain.rs`, `docs/changelog.md` | Throwaway mux + GUI, local split first so ids diverge: GUI ClientPanes 4 and 5 were mux 1 and 2. Before: `split-pane --pane-id 4 --move-pane-id 5` against the GUI socket returned `pane 5 not found`. After: that command exits 0; mux pane 2 sits in pane 1's tab at left_col 40; GUI lists pane 5 once, in the same tab as pane 4, and the emptied window is gone. |
| `mux-socket-buffers` | `mux-socket-buffers` | `upstream/main`, 11 commits behind at `4fbd6b8e9` | both | wip, not merged into `build`, ours | Attaching to a mux with many panes shows blank panes and freezes on the first keystroke. Raising the unix socket buffers removes the need for the sysctl LaunchDaemon and the launcher's refuse-to-attach check. Before it can merge: rename the branch `unit/mux-socket-buffers`, rebase it, drop its `WEZTERM_UDS_DEBUG` readback block. It takes effect only when the mux server is restarted | `Cargo.lock`, `wezterm-uds/Cargo.toml`, `wezterm-uds/src/lib.rs` | With `net.local.stream.recvspace` lowered — the sockbuf LaunchDaemon's 1 MB default masks the defect, so the proof must run without it — attach to a mux holding many panes: the stock build wedges with the socket Recv-Q pinned at the 8 KiB cap (`netstat -a -f unix`), this unit does not |

## Decisions

**`tab_bar_rows = "auto"` was considered and declined.** Letting the row count
follow the tab count sounds obviously right and is not. The row count is part of
the window's geometry, so changing it resizes the terminal area and reflows every
pane in the focused tab — on the machine this fork was built for, a mux round
trip across 39 panes — and the trigger would be *opening a tab*, an action taken
without looking at the bar. Near any threshold it also flaps, and the fix for
flapping is a hysteresis constant standing in for a property nobody has named.
The property actually wanted is "can I recognise this tab", which depends on
which characters survive rather than how many; the answer to that is grouping
tabs by repo so the name is said once, not more rows. If a row count must change
on its own, the defensible trigger is a window resize or a display change, not a
tab count — and better still is a key that fits the rows on demand, which needs
no engine change at all.

Two layouts were sketched against the live 26-tab session and are worth naming
before anyone re-derives them. **Grouped rows**: tabs clustered by repo, the name
shown once per group and each tab carrying only its slot suffix, groups never
split across rows — it buys legibility by writing less rather than by allocating
more width, and it needs the engine to let Lua say which row a tab belongs on.
**Two-tier**: one chip per project on the first row, the focused project's tabs
on the second — the widest labels of the three and the only one that hides tabs,
which is the wrong trade for a workflow that uses 26 open tabs as spatial memory.
Both need a Lua-facing row assignment; neither is worth building until two plain
rows stop being enough.
