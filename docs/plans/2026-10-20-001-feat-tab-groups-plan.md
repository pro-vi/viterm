---
title: Project folders in the tab bar
objective: Tabs of one project sit together and fold into one stacked tab that opens as a popup, so a 45-tab bar reads by project and every tab stays one Cmd+number away
type: feat
status: completed
builder: cold
date: 2026-10-20
origin: conversation (slots + tab folders mission), the design page https://claude.ai/artifact/LdjTkZeVmTakNbDaewK3B7 (option D, the popup, picked by the owner; page version 8 made the popup follow the tab-bar look)
---

# Project folders in the tab bar

## Objective and background

The owner runs 40 to 50 tabs in one ViTerm window, several per project
(`philemon`, `philemon-1` … `philemon-3`; `dota-market` and 7 slots). The bar
draws them in creation order, so one project's tabs are scattered and titles
are cut short. The design page drew four ways to fold a project's tabs; the
owner picked D: a project with two or more tabs is one stacked tab in the bar,
and clicking it opens a popup over the bottom of the pane listing that
project's tabs. Nothing of it is built. Where a tab belongs comes from
`~/.local/state/slots/map.json`, which bootstrap's `slots map` writes (schema
`slots_map.v1`: rows of `path`, `project`, `kind` ∈ {slot, plain, worktree},
`slot`, `owner`; 135 rows on 2026-10-20).

Three repos change. This repo (the engine): new upstream-shaped units.
Bootstrap (`~/Development/_setup/bootstrap`, the WezTerm config at
`configs/wezterm.lua` and `configs/wezterm/*.lua`): everything that knows
about projects. wezterm-attention (`~/Development/_projs/wezterm-attention`,
the plugin that draws each tab's attention glyph): one exported function.

Facts the design rests on, read in source on `build` at `e6860c4c6` and
bootstrap `0b854172`:

- The GUI reads a tab's working directory from its own client copy, with no
  round trip (`wezterm-gui/src/termwindow/mod.rs:316-325`,
  `wezterm-client/src/pane/clientpane.rs:595-596`). The server sends the full
  directory in every render push (`wezterm-mux-server-impl/src/sessionhandler.rs:141,154`),
  but a client pane starts with none (`wezterm-client/src/pane/renderable.rs:112`)
  and `ClientPane::new` ignores `PaneEntry.working_dir`
  (`wezterm-client/src/domain.rs:759,771`). Inferred, not run: after a GUI
  relaunch a quiet background tab has no directory until it prints.
- Cmd+1…9 on this machine is Lua (`configs/wezterm.lua:1660-1717` in
  bootstrap): it activates `tabs_with_info()` entry `number - 1`. The drawn
  number, the Cmd target and a click target are one index
  (`termwindow/mod.rs:3548-3578`, `tabbar.rs:647`, `mouseevent.rs:463-466`).
  A folder's number range is true only while its tabs are contiguous.
- `Window::set_tab_order` (`mux/src/window.rs:253`, from
  `unit/tab-order-over-protocol`) reorders named tabs in place and keeps the
  active tab; the caller raises `WindowTabOrderChanged`
  (`termwindow/mod.rs:2365` is the one caller today). Lua can only move the
  active tab (`act.MoveTab`), so regrouping 40 tabs from Lua today means 40
  activate-and-move steps.
- No channel exists from Lua to the bar for per-tab data, a hidden tab or a
  click target (`tabbar.rs:70-128`, `:619-647`). `window:set_right_status`
  (`wezterm-gui/src/scripting/guiwin.rs:101-110`, notification
  `SetRightStatus`, setter at `termwindow/mod.rs:1772-1781`) is the template
  for per-window state set from Lua.
- `Modal` (`termwindow/modal.rs:8-28`, painted after the bar at
  `render/paint.rs:271-278`) is the box-model overlay with key focus.
  `Modal::mouse_event` is declared and never called; while a modal is open a
  key that matches no binding is swallowed (`keyevent.rs:729-734`).
- Any change of the hovered UI item rebuilds the whole tab bar
  (`mouseevent.rs:243-245`); a rebuild costs 26–28 ms at 41 tabs
  (`BACKLOG.md:18-26`).
- The attention plugin publishes the drawn tab order only after every tab has
  passed through `format-tab-title` (`wezterm-attention plugin/format.lua:267-276`).
- `wezterm.GLOBAL` and GUI tab ids do not survive a GUI restart; tab order
  survives one only once `unit/tab-order-over-protocol` runs in both the GUI
  and a restarted mux server (`PATCHES.md` row). The running mux server is an
  older engine.
- `update-status` runs every 3000 ms (`configs/wezterm.lua:1894` in bootstrap).

### Requirements

- **R1** A project with two or more tabs in a window is one folded item in the
  bar showing its first and last tab numbers, the project name and a summary of
  its tabs' attention glyphs. A project with one tab is an ordinary tab.
- **R2** Clicking a folded item opens a popup over the pane, next to the item,
  listing the project's tabs drawn in the window's tab-bar look. Picking one
  activates it and closes the popup; Esc, a click elsewhere or a second click
  on the item closes it. One popup at a time.
- **R3** Cmd+number and the drawn numbers still reach every tab, folded or
  not; a folder's numbers are consecutive.
- **R4** The folded item holding the focused tab is marked.
- **R5** Inside a folder: slot checkouts by slot number (a plain clone counts
  as a primary), then worktrees, then by age.
- **R6** Alt+Shift+F opens or closes the popup of the focused tab's folder.
- **R7** Folder order survives a GUI restart; tab order is computed from it.
- **R8** A tab's project comes from `map.json`, refreshed every minute.
- **R9** Dragging a folded item or a tab in the bar moves its project's place.
- **R10** Opening, closing and regrouping move by animation, not by a jump.
- **R11** The running session is not disturbed while this is built, and a
  config that uses the new methods still loads on an engine without them.
- **R12** Each engine change is a unit by this repo's rules (`CLAUDE.md`
  *Adding a change*): one change on its base, a `PATCHES.md` row, docs for
  what Lua can see, a test that fails without it.

Out of scope: tab names (the automatic namer is separate work), the design
page's options A–C, a group gap wider than the look's gap (under D every pair
of neighbouring bar items is two projects, so it would only widen every gap,
which the look already sets), saving which folders are open (under D a
folder is open only while its popup shows, so the page's "which folders are
open" has nothing to save), and changing what the Alt+O picker calls a
project.

## Naming Ledger

| Role / meaning | Existing repo term | Chosen name | Owner / placement | Status | Second consumer / reason | GR6 sibling disposition |
|---|---|---|---|---|---|---|
| A run of adjacent tabs drawn as one item | none (`show_tabs_in_tab_bar` hides all) | tab group | engine | new | bar drawing, popup, drag, `tab_bar_fit` | user-facing prose says "folder"; code says tab group everywhere |
| Lua sets a window's groups | `set_right_status` | `window:set_tab_groups(groups)` | `wezterm-gui/src/scripting/guiwin.rs` | new | bootstrap config | mirrors `set_right_status` |
| Lua sets a mux window's tab order | `Window::set_tab_order` (Rust) | `MuxWindow:set_tab_order(tab_ids)` | `lua-api-crates/mux/src/window.rs` | reuse name | bootstrap config | same name as the Rust method it calls |
| The folded item in the bar | `TabBarItem::Tab` | `TabBarItem::TabGroup` | `wezterm-gui/src/tabbar.rs` | new | fancy bar, mouse, drag, motion | every match on `TabBarItem` lists it; no wildcard arm |
| The popup listing a group's tabs | `CommandPalette`, `PaneSelector` (`Modal`) | `TabGroupPopup` | `wezterm-gui/src/termwindow/tabgrouppopup.rs` | new | opened by click and by Lua | a `Modal` like its siblings |
| Lua opens/closes the popup for the active tab's group | none | `window:toggle_tab_group_popup()` | `guiwin.rs` | new | bootstrap Alt+Shift+F | guarded like `tab_bar_fit` |
| Checkout path → project | `cmd_map` rows | `project_map` | `configs/wezterm/project_map.lua` (bootstrap) | new | `tab_groups`; later the Alt+O picker | reads `map.json`, never writes it |
| Tab arrangement: project per tab, order, groups, saved order | none | `tab_groups` | `configs/wezterm/tab_groups.lua` (bootstrap) | new (name from the design page) | `wezterm.lua` update-status | pure module with a spec |
| Saved folder order | none | `tab-groups.json` | `~/.local/state/viterm/` | new | read at GUI start | keyed by project, never by tab id |
| A tab's visible attention, exported | `resolve_visible_attention` (internal) | `visible_attention(pane_ids)` | wezterm-attention `plugin/init.lua` | new export | bootstrap's folder summary | same projection the renderer uses |

## Surface Direction

- **Origin:** the design page (option D) and page version 8 (popup follows the look).
- **Visitor mode:** operate.
- **Concept:** the bar never moves when a folder opens; the folder opens as a small floating piece of the bar.
- **Preserved / refused:** preserved: each look's tab shape, colors and active marker, the attention glyphs, the tab numbers. Refused: a soft shadow (the engine draws none anywhere), a second row, resizing panes.
- **Observations:**
  - D1: A project with two or more tabs shows as one item whose shape has layered edges behind it, reading its first and last tab numbers, then the project name, then its attention summary.
  - D2: A project with one tab shows exactly as an ordinary tab does today.
  - D3: The folded item holding the focused tab is drawn the way the active look draws the focused tab.
  - D4: Opening a folder moves no item in the bar and does not change the panes' size; the popup covers the bottom lines of the pane, next to its item, with a point toward the item.
  - D5: The popup's background is the bar's background, its corners take the active look's tab shape, and its edge and point are a thin line in the opened item's color.
  - D6: Each popup entry looks like a tab of the bar in the active look and carries the same number, glyph and title that tab would show in the bar.
  - D7: The numbers in a folder's range are the numbers Cmd uses to reach those tabs.
  - D8: When a folder opens its tabs come out of the item into the popup and go back when it closes; a tab joining a folder travels to its place.

## Architecture Decision

**Approach:** Lua decides, the engine draws. Bootstrap owns every project
fact: a tab's project (C6), the tab order and the saved folder order (C4), and
each group's label (C4, C5). It hands the engine two generic facts through two
new methods: the order (C2, one call instead of one move per tab) and the
groups (C1, adjacent runs with a label). The engine owns the folded item, the
popup (C3), clicks, keys, dragging (U10) and motion (U11), and knows nothing
about projects or `map.json`, so every engine unit stays sendable upstream.

**Rejected alternative: grouping inside the engine** — the engine takes a
group key per tab (a `format-tab-title` return field or a new callback), sorts
tabs itself and saves folder order in `DATA_DIR`. It loses on consistency and
maintainability: project identity, the in-folder rank and the saved order are
policy that cannot go upstream, every change to them needs an engine build, a
callback on the rebuild path adds Lua calls to a rebuild that already costs
26–28 ms, and a `format-tab-title` field changes the callback the attention
plugin owns.

**Trade-offs:** grouping lags a change by up to one `update-status` tick
(3 s): a new tab appears at the end and then moves into its folder (D8
animates that move). Moves the owner makes by hand inside a folder are undone
on the next tick, because the order is computed; moving a tab past another
project moves its whole folder (C4).

**Approval criteria:** by approving, the owner agrees that (1) the engine gets
three Lua methods and a folded bar item with no knowledge of projects; (2) the
tab order is recomputed every tick from the saved folder order and the slot
rank, overriding hand moves inside a folder; (3) folder order is saved in a
file keyed by project name, written only by the `human-main` GUI.

**Probed:** none at planning. Two premises are checked at contact before the
units that rest on them (Build Execution Contract › Verify at contact: the
quiet-tab directory, window capture).

## Program obligations

- O1: Every tab still passes through `format-tab-title` on every rebuild, folded or not (the attention plugin's publication and the popup's entries depend on it).
- O2: Hovering popup entries never rebuilds the tab bar.
- O3: Every `match` on `TabBarItem` names `TabGroup`; no wildcard arm stays where a group could fall through (`render/fancy_tab_bar.rs:490`, `:645-646` today).
- O4: Bootstrap calls each new engine method only after probing that the method exists (pattern `configs/wezterm.lua:84-88`), and a missing method leaves today's behaviour.

## Contracts

### C1. `window:set_tab_groups(groups)`

Owner: engine (`guiwin.rs` method → `TermWindowNotif::SetTabGroups` → a field
on `TermWindow`, compared in the rebuild check at `termwindow/mod.rs:2098` so a
change rebuilds the bar and an unchanged call does not).

- Input: a list of `{ tabs = {tab_id, …}, label = string }`. A non-list, a
  non-integer id or a non-string label is a Lua error naming the entry. Tab ids
  not in this window are ignored. A tab named by two groups belongs to the
  first. An empty list clears the groups. Each call replaces the whole set.
  Kept in memory per GUI window; nothing is saved.
- A group folds when, at the moment the bar is built, it has two or more tabs
  in this window and they are contiguous in the window's tab order. A group
  that does not fold draws its tabs as ordinary tabs; nothing is reported.
- A folded group is one `TabBarItem::TabGroup` in place of its tabs. Its text
  is its tab-number range (the members' indices, following
  `show_tab_index_in_tab_bar` and `tab_and_split_indices_are_zero_based`,
  as the bar numbers tabs) followed by `label`. It is drawn with the active
  look (`TabLook`), plus layered edges behind it (D1); when it holds the active
  tab it takes the look's active treatment (D3).
- O1 holds: titles are still computed for every tab, and the folded members'
  titles stay in `TabBarState` for the popup.
- `window:tab_bar_fit()` (fork `tab-bar-fit`) reports drawn items: a folded
  group counts as one item.
- Evidence at planning: source read. Risk: the merge with `tab-bar-rows`,
  `tab-bar-fit` and `fancy-tab-bar-style` is in `fancy_tab_bar.rs` and
  `tabbar.rs`, all four touching item layout.

### C2. `MuxWindow:set_tab_order(tab_ids)`

Owner: engine (`lua-api-crates/mux/src/window.rs`). Calls
`Window::set_tab_order(&ids)` on the window, raises
`MuxNotification::WindowTabOrderChanged(window_id)` when it returns true (the
same notification `move_tab` raises, so the order reaches a server that
understands it), and returns that boolean. Ids not in the window are ignored
(the Rust method's rule). Base unit: `unit/tab-order-over-protocol`.

### C3. The group popup

Owner: engine (`TabGroupPopup`, a `Modal`).

- Opens for one folded group: by a left click on its item, or by
  `window:toggle_tab_group_popup()`, which opens the popup of the group holding
  the active tab, closes the popup if one is open, and returns false when the
  active tab is in no folded group. Opening a second group's popup replaces the
  first.
- Placement: over the panes, next to its item on the side away from the
  window edge the bar sits on (the bar is at the bottom here, so above), its
  point aimed at the item, kept inside the window horizontally (D4).
- Entries: the members in tab order, each drawn by the same element code and
  `TabLook` as the bar's tabs, from the titles already in `TabBarState` (no Lua
  call). Box per D5; the point needs one new corner shape beside the existing
  ones in `render/corners.rs`.
- Closing: a left click on an entry activates that tab, then closes. Esc
  closes. A click on the open item closes. A press anywhere outside the popup
  closes it; that press is consumed, except in the tab bar, where it then acts
  as it would with no popup (another item opens its popup, a tab activates).
  The popup also closes when its group stops folding or the window's groups
  change so the group is gone.
- Keys while open: Esc is taken; a key with an assignment runs it (Cmd+number
  included); any other key is swallowed, as the palette does
  (`keyevent.rs:729-734`).
- O2 holds.

### C4. Tab arrangement (bootstrap `tab_groups`)

Owner: bootstrap `configs/wezterm/tab_groups.lua`, a pure module (no `wezterm`
global; inputs passed in), called from `update-status` once per window per tick.

- **A tab's project key:** the local path of the tab's first pane (the pane
  with index 0 in `panes_with_info`, the top-left one) through C6. A mapped path
  gives the row's `project`. A local path no row covers gives `dir:` plus its
  last path component. A pane with no directory or a remote host keeps the key
  this GUI last saw for that tab id; with none seen, the key is nil.
- **Rank inside a folder:** `slot` rows by slot number, a `plain` row as slot
  0, then `worktree` rows, then unmapped tabs; ties by tab id (creation order
  within one GUI process).
- **Folder order:** the saved order (C4 file) with keys it lacks appended in the
  order they first appear in the window.
- **Desired order:** tabs sorted stably by (folder order of key, rank, tab id).
  A tab with a nil key rides directly after the tab before it in the current
  order and is in no group.
- **Apply:** when the desired order differs from the current one, one
  `set_tab_order` call (C2). A tick never activates a tab.
- **Groups:** every key with two or more tabs in the window is one group, its
  tabs in desired order, its label the project name (`dir:x` shows `x`) and,
  when any member shows attention, a space and the summary: for each distinct
  indicator among the members (C5), the indicator followed by its count,
  ordered by the plugin's priority, highest first. `set_tab_groups` is called
  only when the groups or a label changed.
- **Saved folder order:** `~/.local/state/viterm/tab-groups.json`,
  `{ "schema": "viterm_tab_groups.v1", "order": [key, …] }`. Read once when a
  window is first arranged in this GUI process; written (temp file, then
  rename) only after that window's order has been applied, and only when the
  order changed. The new order is the keys present in the window in their
  current order, with each absent key kept right after the present key that
  preceded it before. Only the GUI attached to `human-main` reads or writes it;
  every other GUI arranges from the window's own order.
- **Invariant:** `saved order written` ⇒ `this window was arranged since this GUI
  process started`, so the creation order a restarted GUI sees can never
  overwrite the saved order.

### C5. `visible_attention(pane_ids)` (wezterm-attention)

Owner: wezterm-attention `plugin/init.lua`, public. Takes GUI pane ids
(numbers), maps each through the same key lookup the renderer uses
(`drawn_pane_key`), and returns exactly what `resolve_visible_attention`
returns for a tab with those panes, including the winning priority as
`priority`. A pane with no cached entry contributes nothing. Bootstrap's
`external/registry.json` entry for wezterm-attention raises `needs` to the
release carrying it, in U7's commit.

### C6. Reading `map.json` (bootstrap `project_map`)

Owner: bootstrap `configs/wezterm/project_map.lua`, pure.

- Location: `$SLOTS_STATE_DIR/map.json`, default `~/.local/state/slots/map.json`
  (mirror of `bin/slots:105`; a test pins the two).
- Read every tick; parsed only when the text differs from the last text read.
  Parse refuses (keeps the last good map, or none) when `schema` is not
  `slots_map.v1` or `checkouts` is not a list; a row without a string `path` and
  `project`, or with a `kind` outside the three, is skipped. `slot` and `owner`
  may be absent (a JSON null parses as absent).
- Local path from a pane's `current_working_dir`: the host is empty,
  `localhost` or this machine's hostname; use `file_path` (already decoded;
  never decode again); strip one trailing `/`; anything else gives nil.
- Lookup: the row with the longest `path` that equals the local path or is
  followed in it by `/`.

### C7. Operating limits

- Mechanism: the arrangement makes at most one `set_tab_order` and one
  `set_tab_groups` call per window per tick, and none when nothing changed.
- Probe: with 45 tabs, one tick of arrangement adds at most 5 ms to
  `update-status`; kill condition above 5 ms → cache each tab's key by its
  first pane id and recompute only when that pane's directory text changes.
- Probe: `tests/wezterm-mux-server/gui_lane.sh` still passes its 5 ms per
  rebuild budget on the integrated build with groups set.

### Shared scenarios

- **regroup-new-tab:** a window with tabs `philemon`(slot 0), `x`, `philemon-2`
  (slot 2) → one tick → order `philemon`, `philemon-2`, `x`; one group of two
  labelled `philemon`; the stack reads `1–2: philemon`. Owners: U5 (order and
  groups), U7 (calls made), U6 (drawn).
- **restart-keeps-order:** saved order `[x, philemon]`, a fresh GUI whose
  window lists `philemon`, `x`, `philemon-2` → first tick → order `x`,
  `philemon`, `philemon-2`; the file is unchanged. Owners: U5, U7.
- **quiet-tab-after-relaunch:** a background tab that has printed nothing since
  the GUI attached has a directory in Lua. Owner: U2.

## Implementation Units

### U1. Lua can set a mux window's tab order

- **Goal:** one Lua call puts a window's tabs in a given order (C2).
- **Requirements:** R7, R12
- **Direction:** none
- **Dependencies:** None (base `unit/tab-order-over-protocol`)
- **Files:** Modify `lua-api-crates/mux/src/window.rs`; Create `docs/config/lua/MuxWindow/set_tab_order.md`; Modify the MuxWindow index in `docs/config/lua/MuxWindow/index.md` if one lists methods; this repo's `PATCHES.md` row at integration (U12).
- **Approach:** a method beside `tabs_with_info` (`window.rs:75-95`): resolve the window, call `set_tab_order`, notify `WindowTabOrderChanged` when it returns true, return the boolean.
- **Patterns to follow:** `termwindow/mod.rs:2346-2365` (`move_tab` raising the notification); `docs/config/lua/window/tab_bar_fit.md` for the doc page.
- **Test scenarios:** *Happy path:* tabs a, b, c; call with `{c, a}` → order c, b, a; returns true; the notification was raised. *Edge:* an id not in the window → ignored, others reorder. *Edge:* the current order → returns false, no notification.
- **Verification:** the scenarios hold; `cargo test -p mux` and the crate's own tests pass on the unit branch; `cargo fmt --all -- --check` passes.
- **Proven through:** a Rust test on `mux::Window` for the order (exists in the base unit) plus a throwaway GUI whose config calls the method at `gui-startup`, read back with `wezterm cli list` against its own socket.
- **Runtime evidence:** unverified — the throwaway GUI run in Checkpoint settles it.
- **Checkpoint:** auto — the throwaway GUI shows the new order in `wezterm cli list --format json` order.

### U2. A client pane starts with the server's directory

- **Goal:** the GUI knows every pane's directory from the moment it attaches (shared scenario quiet-tab-after-relaunch).
- **Requirements:** R7, R8, R12
- **Dependencies:** None (base `upstream/main`)
- **Files:** Modify `wezterm-client/src/domain.rs` (`ClientPane::new` call sites at 759, 771, 1086), `wezterm-client/src/pane/clientpane.rs`, `wezterm-client/src/pane/renderable.rs`; Test beside them if a constructor test is possible, else the harness below.
- **Approach:** pass `PaneEntry.working_dir` into the new pane's `RenderableInner.working_dir`, and refresh it on resync when the entry carries one. Render pushes keep overwriting it as today.
- **Patterns to follow:** how `ClientPane::new` takes the entry's title.
- **Test scenarios:** *Happy path:* a mux with a background tab whose shell printed before the GUI attached → the GUI's Lua sees its `current_working_dir` immediately. *Edge:* an entry with no directory → nil, as today.
- **Verification:** quiet-tab-after-relaunch holds on a throwaway mux and GUI.
- **Proven through:** a throwaway mux server and GUI pair built from the unit branch, per this repo's `CLAUDE.md` *Trying a change*; a config logging `current_working_dir` of every tab at `gui-attached` + 2 s.
- **Runtime evidence:** unverified — the premise is checked first (Verify at contact); the harness settles the fix.
- **Checkpoint:** auto — the log shows a directory for the quiet tab with the unit and none without it.

### U3. The attention plugin exports a tab's visible attention

- **Goal:** bootstrap reads the same glyph per tab that the plugin draws (C5).
- **Requirements:** R1
- **Dependencies:** None
- **Files (wezterm-attention):** Modify `plugin/init.lua`; Test under `tests/lua/` (the plugin's LuaJIT specs); Modify `CHANGELOG.md`, `README.md` (public API list).
- **Approach:** a public function mapping pane ids through `drawn_pane_key` and returning `resolve_visible_attention`'s projection plus `priority`.
- **Patterns to follow:** `plugin/init.lua:222-235` (`wrap_title_formatter` resolving a tab), `plugin/runtime.lua:930` (`get_attention_view` as a public read).
- **Test scenarios:** *Happy path:* two panes, one thinking and one review → the same indicator and type the renderer draws for that tab. *Edge:* no cached entries → empty indicator, nil type.
- **Verification:** the plugin's spec suite passes with the new cases; the renderer's output is unchanged.
- **Proven through:** the plugin's existing LuaJIT spec harness.
- **Checkpoint:** auto — the plugin's spec run.

### U4. Bootstrap reads the checkout map

- **Goal:** any Lua code can turn a pane's directory into a project row (C6).
- **Requirements:** R8
- **Dependencies:** None
- **Files (bootstrap):** Create `configs/wezterm/project_map.lua`, `tests/wezterm-config/project_map_spec.lua`, `tests/fixtures/slots_map.json`; Modify `tests/slots/test_map.py` (writes the fixture from a synthetic tree and asserts the checked-in copy matches), `scripts/ci-lint.sh` (run the spec), `install.sh` (link the module, beside `tab_themes` at :207), `scripts/doctor.sh` (`check_link`), `CLAUDE.md` Sync Map row.
- **Approach:** pure functions: `parse(text)`, `local_path(url, hostname)`, `lookup(map, path)`. No file access inside; the caller reads.
- **Patterns to follow:** `configs/wezterm/mux_ops.lua:38-45` (cwd decoding), `configs/wezterm/auto_compact.lua:393-459` (pure module taking `json_parse`), `tests/wezterm-config/tab_bar_spec.lua:1-20` (spec harness).
- **Test scenarios:** *Happy path:* a subdirectory of `philemon-2` → project `philemon`, kind `slot`, slot 2. *Edge:* `repo-10` never matches `repo-1`; nested rows → the longest wins; trailing slash; `a%41` in `file_path` stays `a%41`; null `slot`/`owner`. *Error path:* wrong schema → refused with a reason, previous map kept; unknown kind → that row skipped; remote host → nil.
- **Verification:** the spec passes; the fixture parity test fails when the writer's row shape changes.
- **Proven through:** the LuaJIT spec on a fixture the Python writer produced.
- **Checkpoint:** auto — `luajit tests/wezterm-config/project_map_spec.lua` and `python3 tests/slots/test_map.py`.

### U5. Bootstrap computes the arrangement

- **Goal:** from a window's tabs, the map, the saved order and attention, compute the desired order, the groups and the next saved order (C4).
- **Requirements:** R1, R3, R5, R7
- **Dependencies:** U4
- **Files (bootstrap):** Create `configs/wezterm/tab_groups.lua`, `tests/wezterm-config/tab_groups_spec.lua`; Modify `scripts/ci-lint.sh`, `install.sh`, `scripts/doctor.sh`, `CLAUDE.md` Sync Map row.
- **Approach:** one entry function taking plain tables (tabs with id, index, first-pane path, pane ids; the parsed map; the saved order; an attention lookup) and returning `{ order, groups, saved_order }`. No wezterm calls.
- **Patterns to follow:** `configs/wezterm/tab_bar.lua` (pure decision module with a spec).
- **Test scenarios:** *Shared:* regroup-new-tab and restart-keeps-order (order and groups). *Edge:* a nil-key tab rides after its predecessor and is in no group; a one-tab project is in no group; ranks: slot 0, slot 2, plain, worktree, unmapped; a closed folder's key keeps its place in the saved order. *Edge:* a tab whose first pane moves to another repo changes group on the next call. *Error path:* attention lookup missing → labels carry no summary.
- **Verification:** the spec passes.
- **Proven through:** the LuaJIT spec.
- **Checkpoint:** auto — `luajit tests/wezterm-config/tab_groups_spec.lua`.

### U6. The bar folds a group into one item

- **Goal:** `window:set_tab_groups` and the folded item (C1).
- **Requirements:** R1, R3, R4, R12
- **Direction:** D1, D2, D3, D7
- **Dependencies:** None in this plan (base `unit/fancy-tab-bar-style`, for `TabLook`)
- **Files:** Modify `wezterm-gui/src/tabbar.rs` (`TabBarItem::TabGroup`, folding in `TabBarState::new`), `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` (drawing; lift `item_to_elem`/`tab_elem` out of `build_fancy_tab_bar` so U8 reuses them), `wezterm-gui/src/termwindow/mod.rs` (notification, field, rebuild check), `wezterm-gui/src/scripting/guiwin.rs` (method), `wezterm-gui/src/termwindow/mouseevent.rs` (a click on a group: until U8, it activates the group's first tab); Create `docs/config/lua/window/set_tab_groups.md`; Test in `tabbar.rs` / `fancy_tab_bar.rs` test modules.
- **Approach:** fold in the item list after titles are computed for all tabs (O1); a pure function from (tabs in order, groups, active index) to items is the tested seam. O3.
- **Patterns to follow:** `guiwin.rs:101-110` and `termwindow/mod.rs:1772-1781` (`set_right_status` → notif → setter → rebuild); `fancy_tab_bar.rs:1167-1456` (pure tests).
- **Test scenarios:** *Happy path:* tabs 0–5, group {1,2,3} → items tab0, group(1..3), tab4, tab5; range text `2–4` with one-based numbers. *Edge:* a non-contiguous group → its tabs drawn plain; a group of one → plain; an id in two groups → the first wins; zero-based numbers → `1–3`; active tab 2 → the group item is active. *Error path:* a label that is not a string → Lua error naming the entry. *Integration:* titles are computed for folded tabs (count `format-tab-title` calls in a throwaway GUI log).
- **Verification:** the scenarios hold; `tab_bar_fit` counts the group as one item; D1, D2, D3, D7 hold on the throwaway GUI.
- **Proven through:** pure Rust tests on the item function; a throwaway GUI whose config sets two groups.
- **Runtime evidence:** unverified — the throwaway GUI run settles it.
- **Checkpoint:** auto — `cargo test -p wezterm-gui` on the unit branch and the throwaway GUI's capture or, when capture fails (Verify at contact), its logged item list.

### U7. Bootstrap arranges and folds real tabs

- **Goal:** the config runs the arrangement every tick and drives the engine (C4 apply, groups, saved order; C6 reading).
- **Requirements:** R1, R3, R5, R7, R8, R11
- **Dependencies:** U4, U5 (code); U1, U6 for anything to happen (guarded by O4)
- **Files (bootstrap):** Modify `configs/wezterm.lua` (load both modules with the guarded `load_module` at :30-33; call the arrangement in `update-status`; probe `set_tab_order`, `set_tab_groups`), `configs/wezterm/tab_bar.lua` (`decide` key includes the drawn item count; `row_neighbour` maps through groups), `tests/wezterm-config/tab_bar_spec.lua`, `external/registry.json` (wezterm-attention `needs`, C5), `CHANGELOG.md` (vNEXT entry), `docs/plans/BACKLOG.md` (the deferred lanes below).
- **Approach:** per window per tick: read the map text (C6), build the tab table, call `tab_groups`, apply (C2) and set groups (C1) only on change, write the saved order under the C4 conditions. The `human-main` test is the attached domain's name, as `gui-attached` does (`configs/wezterm.lua:191`).
- **Patterns to follow:** `configs/wezterm.lua:84-88` (method probe), `:1848-1852` (atomic write), `:2433-2452` (`fit_tab_bar_rows` caller of `decide`).
- **Test scenarios:** *Happy path:* the throwaway GUI with three tabs in two fake projects (a temp `map.json` via `SLOTS_STATE_DIR`, a temp saved-order path) → after one tick the order and groups of regroup-new-tab. *Integration:* restart the throwaway GUI → restart-keeps-order. *Edge:* an engine without the methods (the installed engine) → the config loads, no error is logged, tabs are untouched. *Edge:* `decide` recomputes rows when the drawn item count changes; `row_neighbour` lands on the group item.
- **Verification:** the scenarios hold; `./scripts/ci-lint.sh` sections for these specs pass.
- **Proven through:** the throwaway GUI from the integrated engine branch, pointed at a bootstrap worktree's config.
- **Runtime evidence:** unverified — the throwaway GUI run settles it.
- **Checkpoint:** auto — `wezterm cli list` order and the logged groups after one tick, then after a restart.

### U8. The folded item opens as a popup

- **Goal:** `TabGroupPopup` and `window:toggle_tab_group_popup()` (C3).
- **Requirements:** R2, R6, R12
- **Direction:** D4, D5, D6
- **Dependencies:** U6 (base `unit/tab-groups`)
- **Files:** Create `wezterm-gui/src/termwindow/tabgrouppopup.rs`; Modify `wezterm-gui/src/termwindow/mod.rs` (open/close, close on group change), `mouseevent.rs` (a click on a group toggles its popup; route presses to the popup; close on an outside press per C3), `render/paint.rs` (nothing new if `paint_modal` suffices), `render/corners.rs` (the point), `scripting/guiwin.rs` (toggle method), `keyevent.rs` only if Esc does not reach the modal; Create `docs/config/lua/window/toggle_tab_group_popup.md`; Test in the new module.
- **Approach:** a `Modal` whose computed element is built from the lifted bar element functions and `TabLook`; its entries carry a new `UIItemType` with the tab index; hover over them invalidates the modal only (O2). Placement and hit-test are pure functions of (item rect, window size, look) and are the tested seam.
- **Patterns to follow:** `termwindow/palette.rs` (`CommandPalette`: modal, framed box at :516-537), `mouseevent.rs:27-35` (`resolve_ui_item` searching from the end).
- **Test scenarios:** *Happy path:* item at bar x=600 near the bottom → popup above it, point at the item's center; click an entry → that tab is active and the popup is gone. *Edge:* item at the far right → popup kept inside the window, point still at the item; group stops folding while open → popup closes; a second item clicked → its popup replaces the first. *Error path:* toggle with the active tab in no group → returns false, nothing opens. *Integration:* Esc closes; Cmd+3 activates tab 3 with the popup open; a press in the pane closes the popup and does not reach the terminal.
- **Verification:** the scenarios hold; D4, D5, D6 hold in every look (`classic`, `strip`, `pills`, `blades`, `statusline` from bootstrap `tab_themes.lua`).
- **Proven through:** pure Rust tests on placement and hit-test; a throwaway GUI whose config opens the popup from a timer.
- **Runtime evidence:** unverified — the throwaway GUI run settles it.
- **Checkpoint:** auto — `cargo test -p wezterm-gui` and the throwaway GUI capture (or logged layout) in each look.

### U9. Alt+Shift+F toggles the focused tab's folder

- **Goal:** the key binding for C3's toggle.
- **Requirements:** R6
- **Dependencies:** U7 (wiring); U8 for an effect (guarded by O4)
- **Files (bootstrap):** Modify `configs/wezterm.lua` (the binding as an `action_callback` probing the method), `CHANGELOG.md`.
- **Approach:** bind only when `show-keys` shows the chord unbound (Verify at contact); on an engine without the method the callback does nothing.
- **Test scenarios:** *Happy path:* `wezterm show-keys --lua` with the config lists Alt+Shift+F. *Edge:* engine without the method → pressing it changes nothing and logs nothing.
- **Verification:** the binding is listed and is the only one on that chord.
- **Proven through:** `show-keys` against the bootstrap worktree config.
- **Checkpoint:** auto — `show-keys` output.

### U10. Dragging an item in the bar moves its project

- **Goal:** a press-drag-release on a tab or a folded item reorders the window; a folded item moves its whole run.
- **Requirements:** R9, R12
- **Direction:** D7
- **Dependencies:** U6 (base `unit/tab-groups`)
- **Files:** Modify `wezterm-gui/src/termwindow/mouseevent.rs` (drag state from a press on an item, a drop position from the hovered item, apply on release), `wezterm-gui/src/termwindow/mod.rs` (apply through `set_tab_order` and raise `WindowTabOrderChanged`), `render/fancy_tab_bar.rs` (a drop marker); Create or Modify the tab bar doc page; Test the pure drop function.
- **Approach:** the new order is a pure function of (items, dragged item, drop slot); a drag shorter than a few pixels is a click. Bootstrap needs no change: C4 reads the new order as the new folder order on the next tick.
- **Patterns to follow:** `move_tab` at `termwindow/mod.rs:2346-2365`.
- **Test scenarios:** *Happy path:* items a, G(b,c), d; drag G before a → order b, c, a, d. *Edge:* drop on itself → no change; a short press → click behaviour; drag a single tab into the middle of a group's run → the tab lands before or after the run, never inside it.
- **Verification:** the scenarios hold; on the throwaway GUI a drag changes `wezterm cli list` order.
- **Proven through:** the pure drop function's tests; the throwaway GUI for the press path where input can be driven, else unverified.
- **Runtime evidence:** unverified — no tool here drives mouse input into a native window; the owner's morning test settles it.
- **Checkpoint:** auto — `cargo test -p wezterm-gui`.

### U11. Bar items and the popup move by animation

- **Goal:** D8: item positions and the popup's entries ease between two layouts.
- **Requirements:** R10, R12
- **Direction:** D8
- **Dependencies:** U8 (base `unit/tab-group-popup`)
- **Files:** Modify `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs`, `tabgrouppopup.rs`, `render/mod.rs` (`update_next_frame_time` scheduling); Test the pure interpolation.
- **Approach:** keep each drawn item's previous rect by key (tab id or group); after a rebuild, draw each item at a position eased from old to new over about half a second, scheduling frames with `update_next_frame_time` (`render/mod.rs:226-230`, the spinner's path). No Lua runs during motion. With the window unfocused frames are not scheduled, so the final layout is drawn at once.
- **Patterns to follow:** `colorease.rs` (easing curves), `tabbar.rs:154-200` (frame scheduling for the spinner).
- **Test scenarios:** *Happy path:* an item moving from x=100 to x=300 is at 100 at t=0, between at t=0.25 s, at 300 from t=0.5 s. *Edge:* a rebuild during motion starts from the current drawn position; an item that disappears keeps its box until its exit ends.
- **Verification:** the scenarios hold; the rebuild budget (C7) still passes.
- **Proven through:** pure tests on the interpolation; a throwaway GUI frame log.
- **Runtime evidence:** unverified — the feel is the owner's judgment (Human inventory).
- **Checkpoint:** auto — `cargo test -p wezterm-gui` and `gui_lane.sh`.

### U12. Integrate, install, and make the config live

- **Goal:** the owner's next ViTerm start runs the folders.
- **Requirements:** R11, R12
- **Dependencies:** all units built and green
- **Files:** this repo: `PATCHES.md` rows (one per unit), `--no-ff` merges into `build`, `fork/upstream/<slug>.md` only if a unit is to be sent (not now); bootstrap: merge the worktree branch into `master`; wezterm-attention: merge into its default branch and release per its `CLAUDE.md`.
- **Approach:** merge in base order (U2; U1; U6, U8, U10, U11); resolve conflicts in each merge commit and name the partner in the row. Build the release engine and install per `CLAUDE.md` *Building and installing* (keeps the old engine as `viterm-engine.app.prev-<version>`). Do not restart the GUI or the mux server.
- **Test scenarios:** *Integration:* the integrated `build` passes `cargo test` for the touched crates and `gui_lane.sh`; the throwaway GUI from the release build runs U7's and U8's scenarios against the bootstrap `master` config.
- **Verification:** the installed engine's `wezterm --version` names the new `build` commit; `codesign --verify` passes; the rollback copy exists; bootstrap `./install.sh doctor` is no worse than before.
- **Proven through:** the release build itself.
- **Checkpoint:** auto — version, signature and rollback copy present.

### U13. The checkout map refreshes every minute

- **Goal:** R8's freshness: a new checkout joins its folder within a minute.
- **Requirements:** R8
- **Dependencies:** None
- **Files (bootstrap):** Modify `scripts/wezterm-checkpoint.sh` (run `slots map`, quietly, before the checkpoint; a failure is logged and does not stop the checkpoint), its test if one covers the job, `CHANGELOG.md`.
- **Approach:** the per-minute LaunchAgent already runs this script (`com.provi.wezterm-checkpoint`); `slots map` takes about 0.2 s and writes atomically.
- **Test scenarios:** *Happy path:* run the script with `SLOTS_STATE_DIR` pointing at a temp dir → `map.json` appears there. *Error path:* `slots` missing from PATH → the checkpoint still writes and the log names the failure.
- **Verification:** the scenarios hold.
- **Proven through:** running the script against temp state directories.
- **Checkpoint:** auto — the two runs.

## Scope Boundaries

- No tab names, no two-colour titles.
- No change to `slots` or to `map.json`'s shape.
- No change to Cmd+number code: the order makes its numbers right.
- No group gap (Objective › Out of scope gives the reason).
- Nothing is pushed to any remote by this plan.

### Deferred to Follow-Up Work

Each gets a lane in bootstrap `docs/plans/BACKLOG.md` in U7's commit:

- `map-project-id`: `map.json` keys a project by its display name, so adding a clone with a shorter name renames the folder and its saved place (`bin/slots:1739-1744`).
- `map-nested-worktrees`: a worktree nested inside a checkout (`.claude/worktrees/x`) matches its parent's row and takes the parent's rank.
- `alt-o-project-column`: the Alt+O picker names a project by the last path component (`configs/wezterm.lua:2153-2157`) while folders use `map.json`.

## System-Wide Impact

- **Interaction graph:** `update-status` (bootstrap) → `set_tab_order` → `WindowTabOrderChanged` → server (when it understands the order) and every GUI's resync; `set_tab_groups` → bar rebuild → `format-tab-title` for every tab (attention plugin) → drawn items; click/drag → engine order change → next tick reads it.
- **Error propagation:** a Lua error inside the arrangement is caught per window and logged once per distinct message; the tick's other work continues. An engine rejecting `set_tab_groups` input raises a Lua error the config catches.
- **State lifecycle:** groups live in GUI memory and are re-sent after a config reload or restart by the next tick; the saved order is one file written atomically.
- **API surface parity:** the engine options file lists `ENGINE_WANTS` in two places (`bin/viterm:44-45`, `scripts/viterm-install.sh:128-129`); this plan adds no option, so neither changes.
- **Unchanged invariants:** Cmd+number semantics; `format-tab-title` called for every tab; the attention plugin's published tab order (now in grouped order); `tab_bar_rows` behaviour when no group is set; an engine without the new methods behaves as today.
- **Integration coverage:** U7's and U12's throwaway GUI runs; nothing drives real mouse input (U10, U8 clicks), so those paths rest on pure tests until the owner tries them.

## Build Execution Contract

- **Closed decisions:** the Architecture Decision; C1–C7; O1–O4; D1–D8; the in-folder rank in C4; folder order saved by project name, written only by `human-main`; no group gap.
- **Builder autonomy:** record and continue: exact Rust names below the ledger; easing curve and duration within 0.3–0.6 s; the layered-edge drawing; log wording; how a group's range dash is rendered; splitting a unit's commit when the fork rules need it; the throwaway configs' content.
- **Verify at contact:**
  - Quiet-tab premise (U2): a throwaway mux with a background tab that printed before a throwaway GUI attached → Lua sees no directory. If it already sees one, drop U2 and note it in the plan.
  - Window capture: `screencapture -x -o -l<window id>` of a throwaway GUI. The machine notes say it failed from an agent shell on 2026-10-16. If it fails, log the computed items and popup layout from the engine at debug level and mark the visual observations unverified for the owner.
  - `drawn_pane_key` maps GUI pane ids as C5 assumes (read `plugin/` before writing U3); if it needs a drawn tab first, export from the poll path instead and say so in C5.
  - Alt+Shift+F unbound (`show-keys --lua`); if bound, pick the nearest free Alt+Shift chord, record it, and tell the owner.
  - `unit/tab-order-over-protocol` and `unit/fancy-tab-bar-style` exist with the code C2 and C1 name.
- **Stop conditions:** a base unit named above is missing; the integrated `build` without these units fails a check that it passes with none of them added (not ours: record and stop integrating, keep units); `codesign` identity `uttr-dev` missing at install (leave the old engine, say so).
- **Authority boundaries:**
  - Never restart, signal or attach to the owner's GUI or mux server; a second GUI on `human-main` is forbidden (it acknowledges attention markers). Fallback: throwaway mux and GUI per this repo's `CLAUDE.md`.
  - Never edit the live bootstrap checkout's config, or the live wezterm-attention checkout, before its unit passes: both take effect in the running GUI on save. Fallback: a worktree for each, merged at U12.
  - Never install a unit-branch build; install only the integrated `build` (U12).
  - Commit to `build` only fork-only files and `--no-ff` merges; another session may hold uncommitted edits in `PATCHES.md`, so stage only this plan's rows (temporary index, `git apply --cached` of the own hunk).
  - One heavy build at a time (`pgrep -x cargo`, `pgrep -x rustc` first), `CARGO_BUILD_JOBS=1`.
  - No pushes.
- **Expected gate map:** U1, U2, U6, U8, U10, U11: `cargo fmt --all -- --check` and `cargo test` for the crates each touches, green on its unit branch. U3: the plugin's spec suite. U4, U5, U7, U9, U13: their specs and `./scripts/ci-lint.sh` sections. U12: integrated tests, `gui_lane.sh`, install checks. No temporary failure is permitted.
- **Human inventory:**
  - *judgment* — does the folded bar and the popup look and feel right in daily use (D1–D8, the motion). Probe artifact: the installed engine with the live config, at the owner's next ViTerm start. Builder's prediction, written before the morning: the stacks read as projects and the popup follows each look; the motion and drag have rough edges. Permitted while pending: everything in this plan. Rollback: rename the engine back (`CLAUDE.md` *Building and installing*).
  - *consent* — standing approval (bootstrap global instructions, 2026-10-16) covers the commits in three of the owner's repos and installing the engine; the owner asked for this build on 2026-10-20 to test the next morning.
  - *calibration* — three picked quantities: the 3 s tick (existing `status_update_interval`; regroup lag), the 0.5 s motion (from the design page), the 5 ms arrangement probe limit (C7). Each changes only feel or speed, not design.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Merging the groups units with `tab-bar-rows`, `tab-bar-fit` and `fancy-tab-bar-style` conflicts in `fancy_tab_bar.rs`/`tabbar.rs` | Resolve in each `--no-ff` merge, name the partner in the row; U12's integrated tests and `gui_lane.sh` gate the result |
| Visual and click behaviour cannot be observed from an agent shell | Pure tests for placement and hit-test; logged layouts; the owner's morning test is the judgment probe |
| Another session works in this repo's `build` at the same time | Stage only this plan's files; check `git status` before each merge; never take over an in-progress merge |
| A wrong arrangement reorders the owner's real tabs every 3 s | U5's spec covers the order rules; U7 runs on a throwaway GUI before the config goes live; the order never activates a tab |
| The saved order file is overwritten by a fresh GUI's creation order | The C4 invariant and restart-keeps-order |
| The running mux server is older than `tab-order-over-protocol` | Order is computed every tick, so a GUI restart restores it from the file without the server |
