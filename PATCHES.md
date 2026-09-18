# Patches carried on `build`

`main` mirrors `upstream/main` and carries nothing. Each change lives on its own
`unit/<slug>` branch, based on `upstream/main`, containing only that change and
written so it could be sent upstream as it stands. `build` is `upstream/main`
plus a merge of every active unit, and every local build comes from `build`.
This file, `CLAUDE.md` and the `.inbox/` notes exist only on `build`; `CLAUDE.md`
is where the rules live, this file is what they produced.

A carried third-party patch is applied with `git am` so authorship stays with
its author, and its row says `carried`. `git cherry upstream/main unit/<slug>`
reports when a unit has landed upstream verbatim; a rework upstream needs this
file edited by hand.

| slug | branch | side | status | why | files | proving test |
|---|---|---|---|---|---|---|
| `tab-bar-rows` | `unit/tab-bar-rows` | gui | local, ours | The fancy tab bar was one row, so with 26 tabs open each title was cut to five or six characters and the bar could only be navigated by position. `tab_bar_rows` makes the bar N rows tall and spreads the tabs over the rows, dividing the width budget by the tabs on one row rather than by the total. Default 1 leaves the existing element tree untouched. | `config/src/config.rs`, `wezterm-gui/src/termwindow/render/tab_bar.rs`, `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs`, `docs/config/lua/config/tab_bar_rows.md` | With 26 tabs and `tab_bar_rows = 2`: the bar draws two rows, tabs 1–14 on the first beside the status areas and 15–26 plus the new-tab button on the second, and the tab hit regions sit in two bands (measured at dpi 144: row 1 y=692 h=48 holding tab_idx 0–13, row 2 y=742 h=48 holding 14–25), so a click on the second row activates the tab under it. Tab widths came out 137–215 px against a one-row cap near 108. A run with `tab_bar_rows = 1` must be pixel-identical to upstream. || `tab-bar-row-status` | `unit/tab-bar-row-status` | gui | local, ours | With more than one row, only the first could show a status area, so the rows below held tabs and nothing else while the status competed with the tabs on row one. `window:set_left_status()` and `set_right_status()` take an optional row number counting from 1; the window keeps one status string per row and `TabBarItem::LeftStatus`/`RightStatus` carry their row. Omitting the number is what every existing config does and means row one. | `wezterm-gui/src/tabbar.rs`, `wezterm-gui/src/termwindow/mod.rs`, `wezterm-gui/src/termwindow/mouseevent.rs`, `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs`, `wezterm-gui/src/scripting/guiwin.rs`, `docs/config/lua/window/set_{left,right}_status.md` | Depends on `tab-bar-rows`. With `tab_bar_rows = 2` and a status written to row 2, both rows show their own left and right runs and the first row's git cells are unchanged; with no row argument anywhere, the bar is identical to before. |

## Not yet started

The units proposed in `.inbox/2026-09-06-viterm-freeze-fork-plan.md` — the
focus-storm fix carried from PR #7763, the tab title memo, the mux socket
buffers, the client move-pane id translation, user-var replay on attach, tab
order over the protocol, and a vertical tab bar carried from upstream — have no
branches yet. Three more, from the session that built the two tab bar units:

| slug | side | why | state |
|---|---|---|---|
| `flash-message` | gui | `window:flash(text)`: large centred numerals over the panes for transient feedback, so the tab-jump digits do not have to live in the status bar. Chosen from four rendered candidates; the spec, with every duration and curve written as a number, is `.inbox/2026-09-18-flash-message-unit-spec.md` | specified, not started. The one open question is whether the 0.96→1 scale can be done without re-shaping text per frame |
| `mux-tab-get-index` | gui | `MuxTab` has no way to report where it sits, while `window:tabs_with_info()` has `index` and `format-tab-title` gets `tab_index`. Everyone reaches for `tab:get_index()` because every neighbouring API has it, and a config that does gets `attempt to call a nil value` at runtime with the reason only in a log file. Cost two real bugs in bootstrap's config on 2026-09-18 | not started. About twenty lines and a docs page, branches off `upstream/main`, and the first thing here worth sending upstream as a PR |

And one decision recorded so it is not rebuilt from scratch:

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
