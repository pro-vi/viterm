# Patches carried on `build`

`main` mirrors `upstream/main` and carries nothing. Each change lives on its own
`unit/<slug>` branch, based on `upstream/main`, containing only that change and
written so it could be sent upstream as it stands. `build` is `upstream/main`
plus a merge of every active unit, and every local build comes from `build`.
This file and the `.inbox/` notes exist only on `build`.

A carried third-party patch is applied with `git am` so authorship stays with
its author, and its row says `carried`. `git cherry upstream/main unit/<slug>`
reports when a unit has landed upstream verbatim; a rework upstream needs this
file edited by hand.

| slug | branch | side | status | why | files | proving test |
|---|---|---|---|---|---|---|
| `tab-bar-rows` | `unit/tab-bar-rows` | gui | local, ours | The fancy tab bar was one row, so with 26 tabs open each title was cut to five or six characters and the bar could only be navigated by position. `tab_bar_rows` makes the bar N rows tall and spreads the tabs over the rows, dividing the width budget by the tabs on one row rather than by the total. Default 1 leaves the existing element tree untouched. | `config/src/config.rs`, `wezterm-gui/src/termwindow/render/tab_bar.rs`, `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs`, `docs/config/lua/config/tab_bar_rows.md` | With 26 tabs and `tab_bar_rows = 2`: the bar draws two rows, tabs 1–14 on the first beside the status areas and 15–26 plus the new-tab button on the second, and the tab hit regions sit in two bands (measured at dpi 144: row 1 y=692 h=48 holding tab_idx 0–13, row 2 y=742 h=48 holding 14–25), so a click on the second row activates the tab under it. Tab widths came out 137–215 px against a one-row cap near 108. A run with `tab_bar_rows = 1` must be pixel-identical to upstream. |

## Not yet started

The units proposed in `.inbox/2026-09-06-viterm-freeze-fork-plan.md` — the
focus-storm fix carried from PR #7763, the tab title memo, the mux socket
buffers, the client move-pane id translation, user-var replay on attach, tab
order over the protocol, and a vertical tab bar carried from upstream — have no
branches yet.
