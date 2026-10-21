---
title: Peek into a folded tab group, then go
objective: The owner can look inside any folded project from the keyboard without leaving the tab they are on or clearing a finished mark, then go to the tab they picked; a folded project says which of its tabs need them
type: feat
status: active
builder: cold
date: 2026-10-21
origin: conversation (slots + tab folders mission, 2026-10-21); spec bootstrap `docs/vantage/tab-groups-keyboard.md` (cells and decisions settled by the owner); drawn flow bootstrap `.research/slot-tabs/ui-sketch-tab-peek.html` (its `step()` is the reference model, hint letters drawn as option B of `ui-sketch-tab-hints.html`)
---

# Peek into a folded tab group, then go

## Objective and background

Tab groups shipped on 2026-10-20 (`docs/plans/2026-10-20-001-feat-tab-groups-plan.md`):
a project with two or more tabs is one folded item in the fancy tab bar,
`5–10: philemon !1 ✓1 ◆1 ◌2`, and its popup lists its tabs. The owner tried it
and turned folding off (bootstrap `TAB_GROUPS = "order"`, commit dd16daa9) for
two reasons. A Cmd+number jump lands on a tab hidden inside a folder, so they
cannot see which tab they are on. And the folder says how many of its tabs need
them, not which.

Going to a tab is not neutral. It clears a finished mark
(`ATTENTION_AUTOCLEAR = { "stop" }` in bootstrap `configs/wezterm.lua`) and
replaces the pane being read. So the owner separated looking from going.
Settled with them in four rounds of sketches:

- Alt+Shift+F (they call it Peek) puts a letter where each folded item's number
  range was and dims the rest of the bar. A folder's letter opens its popup,
  and the active tab does not change. Inside the popup, a tab's letter goes to
  that tab.
- Peek twice opens the folder holding the active tab, which is what Alt+Shift+F
  does today.
- A number for a tab drawn as itself goes there in one press, as today. A
  number inside a folded group opens that folder with the tab marked, and the
  number again goes there.
- Escape leaves everything as it was. Any other key closes and reaches the pane.
- A folded item names the tabs that need attention, number first:
  `5–10: philemon 7! 6✓ 8◆ ◌`.
- Later, by the owner's call: a key that opens the folder of the next tab that
  needs them (bootstrap `docs/plans/BACKLOG.md` `peek-needs-me-key`).

Two repos change. This repo (the engine): four units, all built on
`unit/tab-groups`. Bootstrap (`~/Development/_setup/bootstrap`, the config at
`configs/wezterm.lua` and `configs/wezterm/tab_groups.lua`): which tabs need
attention, and the keys that call the new methods.

Facts the design rests on, read in source on `build` at d9cf45cd3, bootstrap
dd16daa9 and wezterm-attention fc7a157:

- **Who writes a tab's number.**
  - It is written by whoever titles the tab, not by the bar. The default path
    adds `N: ` (`wezterm-gui/src/tabbar.rs:339-347`).
  - This machine's titles come from the attention plugin's formatter, which
    writes `tab_index + 1` itself (wezterm-attention `plugin/format.lua:181`)
    and ignores `tab_and_split_indices_are_zero_based`.
  - A folded item's range ` 5–10: ` is written by the engine
    (`tabbar.rs:127-137`, `tab_group_text`). That repeats the default path's
    numbering rule rather than sharing it.
- **How a folded item is cut.** The fancy bar ignores `tab_max_width`
  (`docs/config/lua/config/tab_max_width.md`). It cuts every item, folded or
  not, to the width the fit gives it, from the right, with `… `
  (`render/fancy_tab_bar.rs:397-420`, `cut_title`; floor `tab_min_width`, 12
  as bootstrap's launcher passes it). A label with attention marks at its end
  loses the marks first.
- **Group specs.** `parse_tab_groups` (`scripting/guiwin.rs:402-434`) reads
  `label` and `tabs` and ignores other fields. So an older engine ignores a new
  field rather than failing.
- **Key routing with a modal open.**
  - A modal gets a key before the key bindings do: `process_key` calls
    `modal.key_down` first and stops on `Ok(true)` (`termwindow/keyevent.rs:322-336`).
  - A key no binding takes reaches `key_event_impl`. There, while a modal is
    open, it goes to `modal.key_down` again and is then dropped
    (`keyevent.rs:729-734`).
  - The modal check is made again at that point, so a modal that cancels itself
    in the first call lets the key reach the pane. Read, not run.
- **The popup today.**
  - `TabGroupPopup` (`termwindow/tabgrouppopup.rs`) is opened by
    `toggle_tab_group_popup(group_idx)` (`termwindow/mod.rs:1951-1968`). It
    names its group by a tab id.
  - It is laid out in `build_fancy_tab_bar` from `group.entries(active_tab)`
    (`render/fancy_tab_bar.rs:1186-1230`) and placed by `popup_origin`
    (`tabgrouppopup.rs:102-118`), always on the pane side of its item.
  - It closes on Escape, on mouse rules (`press_on_popup`,
    `mouseevent.rs:227-244`), and when its group stops folding
    (`mod.rs:2214-2219`). It stays open when the active tab changes.
  - Its docs say other keys do nothing (`docs/config/lua/window/toggle_tab_group_popup.md`).
- **A tab switch rebuilds the bar.** `MuxNotification::WindowInvalidated` calls
  `update_title_post_status` → `update_title_impl` (`mod.rs:1358-1361`,
  `2142-2153`), which builds `TabBarState::new` (`mod.rs:2185`).
- **Quick select's letters and colors.**
  - Quick select (Alt+Q) draws letters from `config.quick_select_alphabet`
    (default `asdfqwerzxcvjklmiuopghtybn`, `config/src/config.rs:1908`; this
    machine does not set it).
  - It draws them in `quick_select_label_fg/bg`, with a fallback written twice
    (`overlay/quickselect.rs:685-690`, `777-782`). Bootstrap sets the colors to
    `#0B0A10` on `#E11D48` (`configs/wezterm.lua:3287-3288`).
- **What the plugin reports.** The plugin's `visible_attention` returns `type`
  (`thinking`, `review`, `stop`, `notify`: its priority list,
  `plugin/init.lua:46`), `glyph` (the state's symbol with no spinner frame) and
  `priority`. Held and quiet are flags on `thinking` and `stop`, not types.
  Bootstrap's `tab_groups.summary` uses `glyph` and `priority` today
  (`configs/wezterm/tab_groups.lua:55-82`).
- **An async window method works in the deferred callback.** The config already
  calls `gui:effective_config()`, which waits on the window, from the callback
  that resolves a two-digit number (`configs/wezterm.lua` `tab_jump_row`,
  reached from the `wezterm.time.call_after` in `tab_jump_digit`).

### Requirements

- **R1** Opening a folder, by letter or by number, activates no tab and
  clears no mark.
- **R2** Alt+Shift+F shows a letter on every folded item. A folder's letter
  opens its popup. Inside the popup, a tab's letter goes to that tab.
- **R3** Alt+Shift+F while letters are showing opens the folder holding the
  active tab.
- **R4** A number for a tab drawn as itself goes there. A number inside a
  folded group whose popup is not open opens it with that tab marked. A number
  inside the open popup's group goes there.
- **R5** Escape closes the letters or the popup with nothing changed. A key
  with no Cmd, Ctrl or Alt closes them and reaches the pane. A key with Cmd,
  Ctrl or Alt runs its binding and leaves them open.
- **R6** Going to a tab, by any path, closes the letters and the popup.
- **R7** A folded item names the tabs that need attention, as tab number then
  mark, most urgent first, then one working mark. When the item is cut to fit,
  the project name shortens first. Marks that still do not fit end in `+n`.
  No mark is cut through.
- **R8** With a bar of two or more rows, the popup covers a bar row, not the
  pane.
- **R9** None of this shows until `TAB_GROUPS = "fold"`. On an engine without
  the new methods, the keys behave as they do today. On an engine without
  marks, folded items show the project name only.
- **R10** Each engine change is a unit by this repo's rules (`CLAUDE.md`
  *Adding a change*, *Ready to send upstream*).

Out of scope: the "needs me" key (deferred, above); a tint on the folded item
from its most urgent tab (never asked for); letters for tabs that are not in a
folded group; two-letter hints.

## Naming Ledger

| Role / meaning | Existing repo term | Chosen name | Owner / placement | Status | Second consumer / reason | GR6 sibling disposition |
|---|---|---|---|---|---|---|
| A letter drawn on a folded item or a popup tab that a key press picks | quick select "label" (`quickselect.rs`, `quick_select_label_fg`) | hint | engine | new | bar, popup, key reading | "label" already names a group's text (`TabGroupSpec.label`), so the letters are hints in code and docs |
| The state in which folded items show hints | `QuickSelectOverlay` (an overlay pane) | `TabGroupHints` (a `Modal`) | `wezterm-gui/src/termwindow/tabgrouphints.rs` | new | Lua method, key routing, bar build, mouse press | a `Modal` beside `TabGroupPopup`; it draws no element of its own |
| How a key is read while hints show | `press_on_popup` / `PressOnPopup` (mouse) | `key_on_hints` / `HintKey { Pick, Close, Leave, Pass }` | `tabgrouppopup.rs` | new | `TabGroupPopup`, `TabGroupHints` | the mouse sibling's shape: a pure decision the modal acts on |
| Quick select's label colors with their fallback | written twice in `quickselect.rs` | `hint_colors(palette)` | `tabgrouppopup.rs` | new | `TabGroupPopup`, the bar's hints | quick select's own two sites stay as upstream wrote them |
| Lua shows the hints | none | `window:show_tab_group_hints()` | `scripting/guiwin.rs` | new | bootstrap Alt+Shift+F | sibling of `toggle_tab_group_popup` |
| Lua opens a group's popup for one of its tabs | `toggle_tab_group_popup` (active tab only) | `window:open_tab_group_popup(tab_index)` | `scripting/guiwin.rs` | new | bootstrap Cmd+number | 0-based index, as `ActivateTab` takes |
| The tab a popup was opened for | none | marked tab (by tab id) | `TabGroupPopup` | new | popup layout, `open_tab_group_popup` | by id, because the config reorders tabs |
| The tab active when hints or a popup opened | none | `opened_on` | both modals | new | the rebuild check that closes them | — |
| A tab's number as the bar draws it | the rule in `tab_group_text` and the default title path | `tab_number(tab_index, config)` | `tabbar.rs` | new | range, marks, default title | the plugin's formatter is a mirror outside this repo (C9) |
| Per-tab attention a group shows | the counts in bootstrap's label text | `marks` (each `{tab, glyph}`) and `busy` on a group spec | `TabGroupSpec`, `parse_tab_groups` | new | the bar's folded item; the "needs me" key later | the config decides which tabs; the engine writes numbers and cuts |
| Which tabs need attention | bootstrap `tab_groups.summary` | `marks_of` (replaces `summary`) | bootstrap `configs/wezterm/tab_groups.lua` | rename | the driver | the owner's word "Peek" stays in prose and the Alt+? legend, never in code |

## Surface Direction

- **Origin:** `/ui-sketch` runs on 2026-10-21 in bootstrap `.research/slot-tabs/`. `ui-sketch-tab-peek.html` has the flow; `ui-sketch-tab-hints.html` has the letter style the owner picked (B).
- **Visitor mode:** operate.
- **Concept:** looking into a folder is a separate, harmless act from going to a tab, done with letters that appear where the numbers were.
- **Preserved / refused:** the popup's look (option D of 2026-10-19) and the bar's layout are preserved. Refused: anything that moves bar items when the hints appear or go; a letter drawn over title text.
- **Observations:**
  - D1: After Alt+Shift+F, every folded item shows one letter where its number range was, and every other bar item is dimmed. No item changes position or width.
  - D2: The hint letters look like quick select's labels (Alt+Q), in the bar and in the popup.
  - D3: After a folder's letter, its popup is open, and the bar still highlights the same active tab as before the key.
  - D4: In an open popup, each tab shows its hint before its own title, and the title still shows its tab number.
  - D5: A popup opened by a tab's number marks that tab differently from the active tab and from its other tabs.
  - D6: With a bar of two or more rows, the popup sits over a bar row next to its item. With one row, it sits over the pane, as today.
  - D7: A folded item reads range, project, then the tabs that need attention as number and mark (`7!`), most urgent first, then one working mark. When the bar is crowded, the project name shortens before any mark goes.

D4 differs from the picked sketch, where the letter replaced the tab's number
inside the popup too. The number in a popup tab's title is written by the
title formatter (the attention plugin), not the engine. Replacing it would make
every formatter learn a new field. Keeping the number costs a few cells and
leaves numbers usable while the popup is open.

## Architecture Decision

**Approach:** the engine owns hints and marks.

- **Hints.** A new modal, `TabGroupHints`, puts letters on the folded items.
  `TabGroupPopup` puts letters on its tabs. Both read keys through one pure
  decision, `key_on_hints`, the way the popup already reads mouse presses
  through `press_on_popup`. The letters come from `quick_select_alphabet`, are
  drawn in quick select's label colors, and are assigned in the order the bar
  draws the items.
- **Marks.** A group spec gains `marks` and `busy`. The engine writes each
  mark's tab number with the same `tab_number` rule as the range, and cuts the
  project name before any mark.
- **Lua.** Lua gets two methods: `show_tab_group_hints()` to enter, and
  `open_tab_group_popup(tab_index)` to open a folder for a number.
- **Bootstrap** decides when to call them and which tabs need attention.

**Rejected:**

- *Letters owned by the config, through Lua key tables (`act.ActivateKeyTable`),
  with the letters passed in each group spec and in the popup call.*
  - An open popup gets every key before key tables do, and drops a key no
    binding takes (`keyevent.rs:729-734`). So the engine's key routing changes
    either way.
  - The letter-to-group mapping would live in Lua, while the order the items
    are drawn in lives in the engine: one mapping with two writers.
  - Quick select is the pattern here: its labels are computed, drawn and read
    by one component.
- *Attention marks written into the label text by the config, as numbers.*
  - The fancy bar cuts items from the right (`cut_title`), so the marks go
    first.
  - The config cannot know the fitted width.
  - Its numbers lag a hand move by up to a status tick, and restate the
    numbering rule a third time.

**Rationale:** this matches the existing patterns (quick select;
`press_on_popup`), keeps one owner for the letter-to-item mapping and one for
tab numbers, and leaves the Lua surface to two methods and two additive spec
fields.

**Trade-offs:**
- The hints use quick select's alphabet and colors, so they cannot be styled
  apart from Alt+Q without a new option.
- Items past the alphabet's length get no hint.
- The popup now closes when the active tab changes, which is a change to the
  shipped `unit/tab-groups` behaviour.
- On an engine without marks, a folded item shows only its project name (R9).

**Approval criteria:**
- The engine, not the config, decides which letter means which item, and
  writes every number the bar shows for a group.
- Opening a folder never activates a tab.
- A popup's tab keeps its number beside its hint (D4).

**Probed:** none. The runtime facts the plan leans on are under Verify at
contact.

## Program obligations

- O1: Hints are drawn on the bar iff the window's modal is a `TabGroupHints`. Opening or closing it rebuilds the bar.
- O2: A popup's marked tab, when set, is a tab of that popup's group, named by tab id.
- O3: While hints or a popup are open, the window's active tab is the one that was active when they opened. A rebuild that finds another active tab closes them.
- O4: The hint for item *k* (0-based, in draw order) is the *k*-th character of the lowercased `quick_select_alphabet`, in both the drawing and the key reading, computed from one list. Defined in C3.
- O5: A key that closes the hints or the popup and is not Escape reaches the pane exactly once. Defined in C4.
- O6: Every number the bar draws for a tab (default title, group range, group mark) comes from `tab_number`. Defined in C9.

## Contracts

### C1. `window:show_tab_group_hints()`

Returns `true` when it opened the hints, and `false` otherwise:

| Before | Result |
|---|---|
| nothing open, at least one folded item drawn | hints open (D1); returns `true` |
| nothing open, no folded item (retro bar, no groups, `TAB_GROUPS ~= "fold"`) | nothing changes; returns `false` |
| hints open | nothing changes; returns `false` |
| a group popup open | hints replace it; returns `true` |
| another modal open (palette, launcher) | nothing changes; returns `false` |

Bootstrap's Alt+Shift+F calls it and, on `false`, calls
`toggle_tab_group_popup()`. That is how a second press opens the folder holding
the active tab (R3). Doc: `docs/config/lua/window/show_tab_group_hints.md`.

### C2. `window:open_tab_group_popup(tab_index)`

`tab_index` is a 0-based integer, as `ActivateTab` takes it. A value that is
not a non-negative integer raises a Lua error naming the method. Returns `true`
when it opened a popup, and `false` when the caller should go to the tab
instead:

| The tab at `tab_index` | Result |
|---|---|
| in a folded group whose popup is not open (hints or another group's popup may be open) | that group's popup opens, replacing them, with this tab marked (D5); returns `true` |
| in the folded group whose popup is open | nothing changes; returns `false` |
| drawn as itself, or no tab at that index | nothing changes; returns `false` |
| another modal open | nothing changes; returns `false` |

`toggle_tab_group_popup` keeps its meaning: it returns `true` when it opened or
closed a popup. The two return values answer different questions, and each
doc says which. Doc: `docs/config/lua/window/open_tab_group_popup.md`.

### C3. Hints

- Alphabet: `config.quick_select_alphabet`, lowercased, one character per
  hint. Item *k* in draw order gets character *k*. Items past the alphabet's
  length get none. An empty alphabet gives no hints, so `show_tab_group_hints()`
  returns `false`.
- In the bar: folded items in the order the bar draws them. The hint replaces
  the ` first–last: ` range. The item keeps the width it has without hints, so
  nothing moves (D1). Every item that is not a folded item is dimmed by one
  factor, a calibration item (Human inventory).
- In the popup: its tabs in tab order, each hint drawn before the tab's own
  title (D4), always, however the popup was opened.
- Colors: `hint_colors`, quick select's `quick_select_label_fg` on
  `quick_select_label_bg` from the resolved palette, with quick select's
  fallback (D2). Doc: `docs/config/lua/config/quick_select_alphabet.md` gains a
  line saying tab group hints use it.

### C4. Reading a key while hints or a popup are open: `key_on_hints`

A pure function of the key, its modifiers and the hints in force, returning one
of four outcomes. Both modals act on it:

| Key | Outcome | `TabGroupHints` does | `TabGroupPopup` does |
|---|---|---|---|
| Escape, no modifiers | `Close` | closes; returns `Ok(true)` | closes; returns `Ok(true)` |
| a hint's character, no modifiers or Shift only | `Pick(k)` | opens item *k*'s popup, unmarked; returns `Ok(true)` | activates tab *k*; returns `Ok(true)` (O3 then closes it) |
| any key with Cmd (SUPER), Ctrl or Alt | `Pass` | stays; returns `Ok(false)` so bindings run | stays; returns `Ok(false)` |
| a modifier key pressed alone | `Pass` | stays; returns `Ok(false)` | stays; returns `Ok(false)` |
| any other key | `Leave` | closes; returns `Ok(false)`, and the key reaches the pane (O5) | closes; returns `Ok(false)`, same |

- Shift alone counts as no modifier for the hint test only. The hints are
  lowercase, so a capital letter is `Leave`.
- Alt+Shift+F with a popup open is `Pass`. The binding then calls
  `show_tab_group_hints()`, which replaces the popup with hints (C1). The popup
  is not closed and reopened.

### C5. Lifetime

Hints and a popup close on:
- Escape;
- a `Leave` key;
- a rebuild that finds a different active tab (O3);
- the last folded item going away (hints), or their group no longer folding
  (popup, as today);
- a mouse press. A press while hints are open closes them, then acts as it
  would with nothing open. A press while a popup is open follows
  `press_on_popup`, as today.

Only one of the two is open at a time, because both are the window's single
modal.

### C6. Popup placement

`popup_origin` puts the popup over the bar row next to its item when the bar
has a row on the side toward the window edge it sits on (below the item for a
bar at the bottom). Otherwise it goes over the row on the other side, still
inside the bar. With a one-row bar it goes over the pane, as today (D6).

### C7. Which tabs need attention (bootstrap `marks_of`)

- A member needs attention when its `visible_attention` `type` is `notify`,
  `stop` or `review`. That is every type of the plugin's priority list except
  `thinking`.
- An entry with no `type`, or a type outside that list, gives no mark.
- `marks` lists those members as `{ tab = <tab id>, glyph = <its glyph> }`,
  ordered by higher `priority` first, then by order in the group.
- `busy` is the `glyph` of the first member whose `type` is `thinking`, or
  absent.
- `label` is the project's display name only.

### C8. Bootstrap key wiring

- Alt+Shift+F: `show_tab_group_hints()`. On `false`, `toggle_tab_group_popup()`
  with today's refusals. On an engine without `show_tab_group_hints`, it does
  exactly what it does today.
- Cmd+number (`tab_jump_activate`): the window's `open_tab_group_popup(number - 1)`
  first. On `false`, or on an engine without the method, it activates the tab
  as today.
- `SHORTCUT_LEGEND`: Alt+Shift+F reads "Peek: letters on folders; a letter
  opens one; twice opens this tab's". Cmd+1-9 adds "a tab inside a folder opens
  the folder first".

### C9. Marks on a folded item (engine)

- **Spec fields.** A group spec may carry `marks`: a list of
  `{ tab = <tab id>, glyph = <string> }`. It may also carry `busy`, a string.
  Both are optional.
  - A mark whose tab is not in the group, or a malformed entry, is an error
    naming the entry, as `parse_tab_groups` reports today.
  - An older engine ignores both fields.
- **Text.** The item reads ` first–last: label m1 m2 … busy `.
  - Each mark is written as `tab_number(tab index)` followed by its glyph.
  - With hints showing, ` x: ` takes the range's place, padded to the range's
    width (C3).
- **`tab_number`.** It is the one numbering rule: index plus one, or plus zero
  under `tab_and_split_indices_are_zero_based`. The default title path and
  `tab_group_text` both call it.
  - The attention plugin's formatter writes `index + 1` on its own (a mirror
    outside this repo). `set_tab_groups.md` says that marks are numbered as the
    bar numbers tabs.
- **Cutting.** When the fit gives a folded item fewer cells than its text needs,
  it is cut in this order:
  1. The label shortens to as little as one character and `…`.
  2. Marks are dropped from the end, and `+n` names how many went.
  3. Only then is anything else cut. The range is never cut.
  
  A mark is never cut through.
- **Docs.** `docs/config/lua/window/set_tab_groups.md` describes `marks`, `busy`
  and the cut order.

### State × action (engine, one window)

| State | `show_tab_group_hints()` | `open_tab_group_popup(i)` | hint key | Escape | plain other key | key with Cmd/Ctrl/Alt | mouse press | active tab changes |
|---|---|---|---|---|---|---|---|---|
| nothing open | hints open if any folded item → `true`; else `false` | popup of *i*'s group, *i* marked → `true` if *i* folded; else `false` | goes to bindings or the pane, as today | as today | as today | as today | as today | nothing |
| hints | `false`, hints stay | popup replaces hints → `true` if *i* folded; else `false`, hints stay | that folder's popup, unmarked; active tab unchanged | hints close | hints close; the key reaches the pane | hints stay; the binding runs | hints close; the press acts as with nothing open | hints close |
| popup (group G, maybe marked) | hints replace popup → `true` | *i* in G → `false`, popup stays; *i* in another folded group → that popup, *i* marked → `true`; *i* not folded → `false` | that tab activates; the popup closes on the rebuild | popup closes | popup closes; the key reaches the pane (today it is dropped) | popup stays; the binding runs | `press_on_popup`, as today | popup closes (today it stays) |
| another modal | `false` | `false` | that modal's | that modal's | that modal's | that modal's | that modal's | that modal's |

Omitted, with reason:
- The window losing focus: nothing changes; a modal survives focus loss, as
  the popup does today.
- A config reload: the modal survives; the next rebuild recomputes the hints
  from the new alphabet and groups.
- `set_tab_order` while a popup is marked: the mark is a tab id, so it follows
  its tab (test `mark_survives_set_tab_order`).

### Shared scenarios

- **"peek then go"**: on tab 20, philemon folded as 5–10 with 6 finished.
  - Alt+Shift+F: hints on folded items, the rest dimmed, 20 still highlighted.
  - philemon's hint: philemon's popup open, 6 still shows `✓`, 20 still active.
  - Tab 7's hint: 7 active, popup closed, 6 still shows `✓`.
  - Exercisers: U2 (hints half), U1 (popup half), U5 (keys from the config). Owner: U1, U2.
- **"number opens first"**: on tab 20.
  - Cmd+5: philemon's popup opens with 5 marked, 20 still active.
  - Cmd+5 again: 5 active, popup closed.
  - Exercisers: U1 (method), U5 (config).
- **"type through"**: philemon's popup open. Typing `y`: the popup closes and
  `y` is in the pane. Exercisers: U1 (popup), U2 (hints).
- **"crowded"**: a three-row bar cut so philemon's item gets 14 cells, marks
  `7!`, `6✓`, `8◆`. The item keeps its range and all three marks it has room
  for, the name shortened first, ending in `+n` for any dropped. Exercisers:
  U7 (engine), U4 (config sends them).

## Implementation Units

U-IDs are stable. U7 was added after U1–U6 were first drafted, so it comes
after them in the list. Merge order is in U6.

### U1. The popup letters its tabs, opens for a tab, and lets keys through

- **Goal:** the popup shows a hint before each tab and reads keys by C4. It
  closes when the active tab changes. `window:open_tab_group_popup` opens it
  for a tab, marked.
- **Requirements:** R1, R2 (popup half), R4 (engine half), R5, R6, R10
- **Direction:** D2, D4, D5
- **Dependencies:** None (branch `unit/tab-group-popup-keys` from `unit/tab-groups`)
- **Files:**
  - Modify: `wezterm-gui/src/termwindow/tabgrouppopup.rs` (`key_on_hints`, `HintKey`, `hint_colors`; `TabGroupPopup` gains marked tab, hints and `opened_on`; `key_down`)
  - Modify: `wezterm-gui/src/termwindow/mod.rs` (open by tab index; a `TermWindowNotif` variant; close on active-tab change in the rebuild)
  - Modify: `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` (popup tabs get a hint element and the mark)
  - Modify: `wezterm-gui/src/scripting/guiwin.rs` (`open_tab_group_popup`)
  - Create: `docs/config/lua/window/open_tab_group_popup.md`
  - Modify: `docs/config/lua/window/toggle_tab_group_popup.md` (keys, hints, closes on a tab change)
  - Test: `wezterm-gui/src/termwindow/tabgrouppopup.rs` (`mod test`)
- **Approach:**
  - `key_on_hints` is pure, next to `press_on_popup`, and is tested the way
    `press_on_popup` is.
  - The popup records the active tab at open and its marked tab id. The rebuild
    check sits beside the existing "group no longer folds" close
    (`mod.rs:2214-2219`).
  - A `Pick` activates the tab through the path a click on a popup tab takes
    (`UIItemType::TabGroupPopup(Some(tab_idx))` in `mouseevent.rs`).
- **Patterns to follow:** `press_on_popup` / `PressOnPopup` (`tabgrouppopup.rs`); the `toggle_tab_group_popup` notification and async method (`guiwin.rs:112-119`, `mod.rs:1232-1237`); quick select's label colors (`overlay/quickselect.rs:685-690`).
- **Test scenarios:**
  - *Happy path:*
    - `key_on_hints(Char('s'), NONE, "asd")` → `Pick(1)`;
    - `Char('S')` with SHIFT → `Leave`;
    - Escape → `Close`;
    - `Char('5')` with SUPER → `Pass`;
    - `Char('y')` → `Leave`;
    - `KeyCode::Super` alone → `Pass`.
  - *Edge case:* hints shorter than the alphabet → only the hints in force are
    live (`Char('f')` with three hints from alphabet `asdf` → `Leave`).
  - *Edge case:* an empty alphabet → no hints, and no panic (`empty_alphabet_yields_no_panic`).
  - *Error path:* `open_tab_group_popup(-1)` and `open_tab_group_popup(1.5)`
    raise (`open_by_index_rejects_bad_index`); `open_tab_group_popup(999)`
    returns `false`.
  - *Integration:* on a throwaway GUI with two groups:
    - `open_tab_group_popup(i)` for a hidden tab returns `true`, the popup
      marks it, and the active tab is unchanged (`peek_does_not_clear_stop`
      checks the active tab id);
    - the same call again returns `false`;
    - a tab drawn as itself returns `false` (`open_returns_false_outside_fold`);
    - activating another tab closes the popup;
    - a `set_tab_order` that moves the marked tab keeps the mark on it
      (`mark_survives_set_tab_order`).
  - *Shared:* "number opens first" (method half); "type through" (popup half).
- **Verification:** C2 and C4's popup column hold; D2, D4, D5 seen on a capture of the throwaway GUI; O2, O3, O5 hold.
- **Proven through:** `key_on_hints` unit tests; the throwaway GUI of `CLAUDE.md` *Trying a change without disturbing the running session*, driven by Lua.
- **Runtime evidence:** unverified. A capture of the throwaway GUI after `open_tab_group_popup` would settle D4 and D5. A typed key reaching the pane (O5) needs a key event into the GUI (Human inventory, assistance).
- **Checkpoint:** auto — `cargo test -p wezterm-gui tabgrouppopup` and `cargo fmt --all -- --check` on the unit branch, then the throwaway GUI capture.

### U2. Hints on the folded items

- **Goal:** `window:show_tab_group_hints()` opens `TabGroupHints`, the bar
  draws hints in place of the ranges and dims the rest, and a hint opens that
  folder.
- **Requirements:** R1, R2, R3 (engine half), R5, R6, R10
- **Direction:** D1, D2, D3
- **Dependencies:** U1 (branch `unit/tab-group-hints` from `unit/tab-group-popup-keys`, because it uses `key_on_hints`, `hint_colors` and the popup's open path)
- **Files:**
  - Create: `wezterm-gui/src/termwindow/tabgrouphints.rs`
  - Modify: `wezterm-gui/src/termwindow/mod.rs` (module, open/close, rebuild check, the bar build passing the hints)
  - Modify: `wezterm-gui/src/tabbar.rs` (`TabBarState::new` takes the hints; `tab_group_text` with a hint keeps the range's width; items carry whether they are dimmed)
  - Modify: `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` (dimmed items; the hint drawn in `hint_colors`)
  - Modify: `wezterm-gui/src/termwindow/mouseevent.rs` (a press closes the hints, then acts)
  - Modify: `wezterm-gui/src/scripting/guiwin.rs` (`show_tab_group_hints`)
  - Create: `docs/config/lua/window/show_tab_group_hints.md`
  - Modify: `docs/config/lua/config/quick_select_alphabet.md`
  - Test: `wezterm-gui/src/tabbar.rs` (`mod test`), `wezterm-gui/src/termwindow/tabgrouphints.rs` (`mod test`)
- **Approach:**
  - The modal draws no element; the bar does the drawing.
  - `TabBarState` equality includes the hints, so opening and closing them
    rebuilds the bar (O1).
  - The hint for item *k* and the character `key_on_hints` matches come from
    one list built from the folded items (O4).
- **Patterns to follow:** `TabGroupPopup` as a `Modal` (`tabgrouppopup.rs:17-73`); `the_bar_draws_a_folded_group_as_one_item_in_place_of_its_tabs` (`tabbar.rs:1182`).
- **Test scenarios:**
  - *Happy path:* a bar with two folded groups and alphabet `"as"` → item texts start with ` a: ` and ` s: `, each as wide as without hints, and plain tabs are dimmed (`hint_letters_follow_quick_select_alphabet`).
  - *Edge case:* more folded items than alphabet characters → the extra items keep their ranges and are not dimmed.
  - *Edge case:* no folded item → `show_tab_group_hints()` returns `false`.
  - *Integration:* on the throwaway GUI:
    - `show_tab_group_hints()` → `true`, and a capture shows D1;
    - again → `false`;
    - `toggle_tab_group_popup()` then opens the active tab's folder;
    - a mouse press on a plain tab closes the hints and activates that tab.
  - *Shared:* "peek then go" (hints half).
- **Verification:** C1, C3 (bar half), C4's hints column and C5 hold; D1, D2, D3 seen on a capture; O1 and O4 hold.
- **Proven through:** `tabbar.rs` tests on `TabBarState::new` with hints; the throwaway GUI.
- **Runtime evidence:** unverified. A capture of the throwaway GUI after `show_tab_group_hints()` would settle it.
- **Checkpoint:** auto — `cargo test -p wezterm-gui` (tabbar and tabgrouphints tests) on the unit branch, then the capture.

### U3. The popup sits over a bar row

- **Goal:** C6.
- **Requirements:** R8, R10
- **Direction:** D6
- **Dependencies:** None (branch `unit/tab-group-popup-placement` from `unit/tab-groups`)
- **Files:**
  - Modify: `wezterm-gui/src/termwindow/tabgrouppopup.rs` (`popup_origin` takes the bar's rect)
  - Modify: `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` (passes it)
  - Modify: `docs/config/lua/window/toggle_tab_group_popup.md` (where it sits)
  - Test: `wezterm-gui/src/termwindow/tabgrouppopup.rs` (`mod test`)
- **Approach:** a pure change to `popup_origin` and its tests. The caller passes the bar's computed bounds.
- **Patterns to follow:** the existing `popup_origin` tests.
- **Test scenarios:**
  - *Happy path:* bar at the bottom, three rows, item on row 1 → the popup's top is below the item, inside the bar.
  - *Edge case:* item on the last row → above the item, inside the bar.
  - *Edge case:* one-row bar → over the pane, as today. A bar at the top mirrors all three.
- **Verification:** C6 holds; D6 seen on a capture of a three-row throwaway bar.
- **Proven through:** `popup_origin` unit tests.
- **Checkpoint:** auto — `cargo test -p wezterm-gui tabgrouppopup` on the unit branch.

### U4. Bootstrap sends which tabs need attention

- **Goal:** C7. Each group spec carries `marks` and `busy`, and its `label` is
  the project name.
- **Requirements:** R7, R9
- **Direction:** D7
- **Dependencies:** U7 for the drawing. Without it, an engine ignores the fields (R9).
- **Files:**
  - Modify: `configs/wezterm/tab_groups.lua` (`marks_of` replaces `summary`; `arrange` puts `marks` and `busy` on each group; `label` is the display name)
  - Test: `tests/wezterm-config/tab_groups_spec.lua`
  - Modify: `CHANGELOG.md` (a `vNEXT` entry)
- **Approach:**
  - The attention passed in keeps `type`, `glyph` and `priority`. `marks_of`
    classifies by `type`, against one named list of the plugin's types.
  - The spec enumerates those four types, so a new type in the plugin shows
    up as a failing case.
- **Patterns to follow:** the existing `summary` and driver tests in `tests/wezterm-config/tab_groups_spec.lua`.
- **Test scenarios:**
  - *Happy path:* members 5 held thinking, 6 stop, 7 notify, 8 review, 9 thinking → `marks` = 7 `!`, 6 `✓`, 8 `◆` (by tab id), and `busy` = the held member's glyph.
  - *Edge case:* nothing needs attention, nothing working → no `marks`, no `busy`, and `label` = `philemon`.
  - *Error path:* an entry with no `type`, or `type = "bogus"` → no mark (`unknown_attention_type_is_refused`).
  - *Integration:* each of the four plugin types is classified, as needing attention or as working.
- **Verification:** C7 holds on the spec; the driver tests pass with the label as the display name.
- **Proven through:** `tab_groups_spec.lua` under LuaJIT.
- **Checkpoint:** auto — `luajit tests/wezterm-config/tab_groups_spec.lua`, and a planted defect (classifying by `glyph`) that fails it.

### U5. Alt+Shift+F and Cmd+number call the new methods (bootstrap)

- **Goal:** C8.
- **Requirements:** R2, R3, R4, R9
- **Dependencies:** U1, U2 for the live behaviour. The config is guarded, so it lands before them without change.
- **Files:**
  - Modify: `configs/wezterm.lua` (Alt+Shift+F binding; `tab_jump_activate`; `SHORTCUT_LEGEND`)
  - Modify: `CHANGELOG.md` (the U4 entry, or its own)
- **Approach:**
  - Each new call goes through `pcall` and a has-method check, as the
    Alt+Shift+F binding does for `toggle_tab_group_popup` today.
  - `tab_jump_activate` gets the GUI window from the mux window, as
    `tab_jump_clear` does.
- **Patterns to follow:** the Alt+Shift+F binding (`configs/wezterm.lua`, the block after `-- Alt+Shift+F opens the popup`); `tab_jump_activate`.
- **Test scenarios:**
  - *Integration:* the config loads on the engine mux server and on Homebrew's (`scripts/wezterm-blink.sh`).
  - *Integration:* on the throwaway GUI running the integrated engine with this config's jump code, Cmd+5 opens philemon's popup with 5 marked, and Cmd+5 again activates 5.
  - *Shared:* "peek then go", "number opens first" (config half).
- **Verification:** C8 holds. On Homebrew wezterm the bindings behave as before: the config loads, and Alt+Shift+F refuses as today.
- **Proven through:** `scripts/wezterm-blink.sh` on both binaries; the throwaway GUI.
- **Checkpoint:** auto — blink on both binaries, then the throwaway GUI check.

### U6. Integrate, install, hand to the owner

- **Goal:** U1, U2, U3 and U7 merged into `build` with ledger rows, the
  integrated engine installed, and the owner able to turn it on.
- **Requirements:** R9, R10
- **Dependencies:** U1–U5, U7
- **Files:**
  - Modify: `PATCHES.md` (four rows; check `git status` first, since other sessions edit it)
  - Modify: `BACKLOG.md` (`tab-groups-live-check` gains the hint, popup and marks cases)
  - Bootstrap: none beyond U4 and U5. `TAB_GROUPS` stays `"order"`.
- **Approach:**
  - Merge `--no-ff` into `build` in the order U7, U1, U2, U3.
  - U2 and U7 both change `tab_group_text` in `tabbar.rs`. The conflict is
    resolved in U2's merge commit, giving C3's hint and C9's marks in one
    composed text.
  - Add each row, then build and install by `CLAUDE.md` *Building and
    installing*, keeping the `.prev-<version>` copy.
  - Run the integrated engine's throwaway GUI with the bootstrap config's tab
    group code, and capture it.
- **Patterns to follow:** the 2026-10-20 plan's U12.
- **Test scenarios:** *Integration:* "peek then go", "number opens first" and "crowded" on the integrated engine's throwaway GUI. Engine and Homebrew blink pass.
- **Verification:** the installed engine's `wezterm --version` names the merge. The owner's running GUI and mux server were not restarted.
- **Proven through:** the throwaway GUI on the installed engine.
- **Checkpoint:** gate — the owner sets `TAB_GROUPS = "fold"` and uses it → their verdict on D1–D7 and on the dim level: continue (done) | re-plan the named observations; unknown: everything is built and installed, nothing else waits.

### U7. Marks on a folded item

- **Goal:** C9. The engine draws `marks` and `busy` with one numbering rule,
  and cuts the project name before any mark.
- **Requirements:** R7, R10
- **Direction:** D7
- **Dependencies:** None (branch `unit/tab-group-marks` from `unit/tab-groups`)
- **Files:**
  - Modify: `wezterm-gui/src/tabbar.rs` (`tab_number`; `TabGroupSpec` gains `marks`, `busy`; `tab_group_text` composes them; the default title path calls `tab_number`)
  - Modify: `wezterm-gui/src/scripting/guiwin.rs` (`parse_tab_groups` reads `marks` and `busy`)
  - Modify: `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` (folded items are cut by C9's order, not by `cut_title`'s right-edge cut)
  - Modify: `docs/config/lua/window/set_tab_groups.md`
  - Test: `wezterm-gui/src/tabbar.rs` (`mod test`), `wezterm-gui/src/scripting/guiwin.rs` (`mod test`), `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` (`mod test`, beside `cut_title`'s)
- **Approach:**
  - A folded item's entry keeps its parts (range, label, marks, busy), so the
    cut can shorten the label first.
  - Every other item still uses `cut_title`.
- **Patterns to follow:** `parse_tab_groups` errors and tests (`guiwin.rs:402-460`); `cut_title` and its tests.
- **Test scenarios:**
  - *Happy path:* group tabs at indices 4–9, marks for the tabs at indices 6, 5 and 7 → text ` 5–10: philemon 7! 6✓ 8◆ ◌ `.
  - *Edge case:* `tab_and_split_indices_are_zero_based` → ` 4–9: philemon 6! 5✓ 7◆ ◌ ` (`label_numbers_match_zero_based`).
  - *Edge case:* cut to 18 cells → the label shortens first, then marks drop from the end with `+n`, and no mark is cut through (`label_budget_uses_column_width`, with `◆` counted by its cell width).
  - *Error path:* a mark naming a tab outside the group, or a mark with no glyph → `set_tab_groups` error naming the entry.
  - *Integration:* a spec without `marks` draws exactly as before (the existing tab group tests stay green).
  - *Shared:* "crowded" (engine half).
- **Verification:** C9 holds; O6 holds (the default title path and the range share `tab_number`).
- **Proven through:** `tabbar.rs`, `guiwin.rs` and `fancy_tab_bar.rs` unit tests.
- **Checkpoint:** auto — `cargo test -p wezterm-gui` on the unit branch, and `cargo fmt --all -- --check`.

## Scope Boundaries

- No change to what a tab's title holds. Its number stays the formatter's.
- No new config option. Hints use quick select's alphabet and colors.
- No hint for a tab that is not in a folded group, and no two-letter hints.
- `TAB_GROUPS` stays `"order"`. Turning on `"fold"` is the owner's.
- No change to the attention plugin. Its `index + 1` numbering is named as a
  mirror in `set_tab_groups.md`, and the plugin's types are enumerated in
  bootstrap's spec.

### Deferred to Follow-Up Work

- The "needs me" key: bootstrap `docs/plans/BACKLOG.md` `peek-needs-me-key`.
- Pills draws folded items' layers in the bar color, so they do not show:
  bootstrap `docs/plans/BACKLOG.md` `pills-stack-edge`.

## System-Wide Impact

- **Interaction graph:**
  - Key routing (`keyevent.rs`) is unchanged. Both modals act through
    `Modal::key_down`.
  - The bar build gains one input (the hints), one per-item flag (dimmed) and a
    structured folded item (marks).
  - The popup's close rules gain "active tab changed".
  - Lua gains two window methods and two optional spec fields.
  - Bootstrap's keys call the methods under guards.
- **Error propagation:**
  - A failing Lua call in the config is caught by `pcall` and shown as today's
    refusal.
  - A bad index or a bad mark raises a Lua error naming it. The config's
    driver already logs `set_tab_groups` errors once per message (bootstrap
    `drive_tab_groups`).
- **Consumers kept unchanged:**
  - `tab_bar_fit` counts the same items, since hints change no widths (U2 bar test).
  - Mouse clicks on folded items and popup tabs behave as today (`press_on_popup` untouched; U2 adds only the hints case).
  - The default title path draws the same numbers through `tab_number` (U7).
- **Wildcard arms that would absorb a new variant without notice:**
  `fancy_tab_bar.rs:725`, the drag match in `mouseevent.rs`, and
  `press_on_popup`'s last arm. This plan adds no `TabBarItem` or `UIItemType`
  variant, so they stay as they are.

## Build Execution Contract

- **Closed decisions:**
  - the Architecture Decision, both rejected alternatives included;
  - C1–C9;
  - O1–O6;
  - D1–D7.
- **Builder autonomy:**
  - the mark's look for D5, within the popup's existing look functions;
  - the dim factor's value, recorded for the owner;
  - names of private Rust helpers;
  - how the throwaway GUI is driven and captured;
  - which one-line PATCHES status each unit ends in, by `CLAUDE.md` *Ready to send upstream*.
- **Verify at contact:**
  - A modal that cancels itself in `key_down` and returns `Ok(false)` lets the
    key reach the pane (`keyevent.rs:322-336`, `729-734`).
    - Check: the U1 throwaway GUI with a key event (Human inventory, assistance).
    - Fallback: in `key_event_impl`, re-read the modal after the bindings and
      send the key to the pane when the modal that was open has closed. Then
      `keyevent.rs` joins U1's files.
  - A tab switch reaches `update_title_impl` before the next frame (`mod.rs:1358-1361`).
    - Check: U1's integration scenario.
    - Fallback: run the O3 check in the `WindowInvalidated` handler too.
  - `unit/tab-groups` on its branch matches what `build` merged.
    - Check: diff the tab group files between the branch tip and `build`.
    - Fallback: base the units on the tip that `build` merged, named in their rows.
  - Captures.
    - The agent's shell cannot capture windows. The owner's AttentionRecorder
      applet (`~/Applications/AttentionRecorder.app`) runs
      `~/.local/state/attention-recorder/job.sh` with screen recording
      granted, and worked on 2026-10-21. A window capture needs
      `screencapture -l <window id>`.
    - Check: one capture of the throwaway GUI.
    - Fallback: the D-IDs stay `unverified` until the owner looks in U6.
- **Stop conditions:**
  - `cargo test -p wezterm-gui` fails on a unit branch in a test the unit did not change.
  - The `uttr-dev` signing identity is missing at install (`CLAUDE.md`).
  - `PATCHES.md` or `BACKLOG.md` carries another session's uncommitted edits in the lines U6 would change.
- **Authority boundaries:**
  - Do not restart the owner's GUI or mux server, and do not attach to `human-main`. Fallback: the throwaway GUI.
  - Do not push any repo. Fallback: report the commits; pushes are the owner's.
  - Do not set `TAB_GROUPS` to `"fold"`. Fallback: the throwaway GUI's own config.
  - Before overwriting `job.sh`, keep a copy and restore it after the capture.
- **Expected gate map:**
  - U1, U2, U3, U7: `cargo fmt --all -- --check` and `cargo test -p wezterm-gui` green on each unit branch, then on `build`.
  - U4: `tab_groups_spec` green, and every spec in `scripts/ci-lint.sh` section f green.
  - U5: blink green on both binaries.
  - U6: all of the above on `build` and on bootstrap `master`.
  - No permitted temporary failures.
- **Human inventory:**
  - **Judgment.** The built look and feel, D1–D7, in daily use.
    - Probe artifact: the installed engine with `TAB_GROUPS = "fold"`, and the
      captures from U1, U2 and U6.
    - Before the owner looks, the builder writes down its prediction that the
      quick-select red letters read as keys at a glance.
    - Work permitted while pending: everything. Blocked: nothing.
  - **Calibration.** The dim factor for non-folded items while hints show.
    - The sketch used 28% opacity. It protects the folders standing out while
      the rest stays readable.
    - It would change the design anywhere from 15% (the rest unreadable) to
      60% (folders don't stand out).
    - Upfront: build with 35% and ask once in the U6 readout.
  - **Calibration.** The hint alphabet is quick select's default
    (`asdfqwer…`), so folders get `a s d f q w`; the sketch used `a s d f g h`.
    - Changing `quick_select_alphabet` changes Alt+Q too.
    - Upfront: name it in the readout; nothing waits on it.
  - **Assistance.** One key event into the throwaway GUI for O5 (a typed
    letter reaches the pane).
    - Not upfront. Without it, O5 is `unverified` until the owner's U6 use.
  - **Consent.** Installing the integrated engine into
    `~/.local/opt/viterm-engine.app`, keeping the `.prev` copy, without
    restarting anything.
    - Covered by the owner's standing approval for work they set going
      (2026-10-16).
    - Effects: the next GUI start runs it.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| The quick-select red, chosen for Alt+Q, reads as an alarm on the bar | Judgment item in U6; a separate color is a later option, not this plan |
| The popup now closes when the active tab changes, which may surprise a click-driven flow | The click path already closes it (`press_on_popup` CloseAndAct); the new rule adds only keyboard paths. U1 integration case |
| Opening and closing hints rebuilds the whole bar | One rebuild each, the same as a hover change today (`mouseevent.rs:243-245`); measured in U6 with `event_timing` |
| `TAB_GROUPS = "order"` hides the whole feature, so the owner sees nothing until they switch | Named in the U6 readout with the one-line change |
| The rollback engine (`.prev`) ignores `marks`, so a folded item there shows only its project name | R9 names it; rolling back is rare and the keys still work |
| U2 and U7 both change `tab_group_text` | Resolved once in U2's merge into `build` (U6 order) |
