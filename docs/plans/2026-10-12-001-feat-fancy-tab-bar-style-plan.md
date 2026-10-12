---
title: The fancy tab bar's look as one config table
objective: A config can choose the shape, spacing and active-tab marker of the fancy tab bar, so ViTerm can switch between tab bar themes
type: feat
status: active
builder: same-session
date: 2026-10-12
origin: conversation (tab bar skins, then a theme system with four presets), 2026-10-12
---

# The fancy tab bar's look as one config table

## Objective and background

The fancy tab bar draws every tab with constants in
`wezterm-gui/src/termwindow/render/fancy_tab_bar.rs`: corners rounded half a
cell on the edge away from the terminal, half a cell of horizontal padding, a
quarter cell vertical, a 0.2 cell gap on the far edge, rows 1.75 title-font
cells tall (`tab_bar.rs`, `tab_bar_pixel_height_impl`). Colors and the font are
the only things a config can change today (`colors.tab_bar`, `window_frame`).
No upstream pull request exposes the geometry; wezterm/wezterm#2789 (a tab bar
line height) was closed.

Four looks were mocked against the live session's 50 tabs at 1920 px
(`/tmp/ui-sketch-tabbar-themes.html`, decisions beside it), and the owner
asked to keep all of them and build "a config for the tab custom theme":

| Preset | Shape | Spacing | Active marker |
|---|---|---|---|
| Strip | square, inactive edge as divider | rows 1.4 cell + 2 px gap | 2 px top line, tint kept |
| Pills | pill | rows 1.9 cell, centered, 4 px gap | 1 px ring, tint kept |
| Blades | slant | rows 1.75 cell, centered, overlapping | fill; title background ignored |
| Status line | square, no divider | rows 1.4 cell, Meslo 11 pt (`window_frame.font`) | reverse-video fill; title background ignored |

The mockup found one defect in today's look: a `format-tab-title` background
(the attention plugin's tint) replaces the active tab's fill, so a thinking
active tab differs from a thinking inactive one only by text brightness.

Requirements:

- **R1** shape: `Rounded` (today), `Square`, `Pill`, `Slant`, with a corner size.
- **R2** spacing: row height, horizontal and vertical padding, a gap between
  tabs that may be negative, and placement in the row (today's edge or middle).
- **R3** active marker: `Fill` (today), `TopLine`, `BottomLine`, `Ring`, with a
  color and width. A marked tab is the same size as an unmarked one.
- **R4** whether a title background replaces the active tab's fill.
- **R5** the table unset draws exactly what is drawn today.
- **R6** bootstrap ships the four presets and a key that cycles them, applied
  with `window:set_config_overrides` only when the engine reports the option.

Scope boundaries: the retro tab bar, the new-tab button and the status areas
keep their shapes (the status areas take the row height); no per-tab shape from
Lua; no images.

## Architecture Decision

One table, `fancy_tab_bar_style`, in `config/src/color.rs` beside
`TabBarStyle` and `WindowFrameConfig`. `build_fancy_tab_bar` resolves it once
into a `TabLook`, which hands the existing tab arms their margin, padding and
corners, and wraps the active tab for a marker other than `Fill`: an outer
element in the marker color whose padding exposes it on the marker's sides,
around the tab element with its padding reduced by the same width.

Rejected: eleven flat `Config` fields (`tab_shape`, `tab_padding_x`, …). A
theme switch would override each key and restore each default by hand; one
table override replaces them together, unset fields falling to defaults.
Rejected for markers: the element's own border. Corner polygons are filled in
the border color of their side (`box_model.rs`, `render_element_background`),
so a ring or line on a rounded, pill or slanted tab would fill those corners
solid.

Consequence: the Lua names in C1 are a public surface; renaming one later
breaks configs that use it.

### Contracts

**C1** `config.fancy_tab_bar_style`, every field optional:

| Field | Values | Default |
|---|---|---|
| `tab_shape` | `"Rounded"`, `"Square"`, `"Pill"`, `"Slant"` | `"Rounded"` |
| `tab_corner_size` | dimension; `Rounded` radius, `Slant` run | `"0.5cell"` |
| `row_height` | number, title-font cells | `1.75` |
| `tab_padding_x`, `tab_padding_y` | dimension | `"0.5cell"`, `"0.25cell"` |
| `tab_gap` | dimension, may be negative | `0` |
| `tab_vertical_align` | `"TerminalEdge"`, `"Middle"` | `"TerminalEdge"` |
| `active_tab_marker` | `"Fill"`, `"TopLine"`, `"BottomLine"`, `"Ring"` | `"Fill"` |
| `active_tab_marker_color` | color | `colors.tab_bar.active_tab.fg_color` |
| `active_tab_marker_width` | dimension | `"2px"` |
| `active_tab_title_background` | bool | `true` |

**C2** marker size: for each side the marker covers, the outer padding is the
marker width `w` and the tab's padding is `max(p - w, 0)`; other sides are
unchanged, so the marked tab's border box equals an unmarked tab's.

**C3** `Pill` corners are half the tab's border-box height on all four corners;
`Slant` cuts the top-left and bottom-right corners over the full height.

## Units

**U1 Config table.** R1–R4. Files: `config/src/color.rs`, `config/src/config.rs`.
Test: an empty table equals `FancyTabBarStyle::default()`; a table with
`tab_shape = "Pill"`, `tab_gap = "-3px"`, `active_tab_marker = "Ring"` parses
with every other field at its default. Checkpoint: auto, `cargo test -p config`.

**U2 Renderer.** R1–R5, C2, C3. Depends on U1. Files:
`wezterm-gui/src/termwindow/render/fancy_tab_bar.rs`, `.../render/corners.rs`,
`.../render/tab_bar.rs`. `TabLook` is pure over the style and the title font's
cell size, so its corners, margins and marker split are tested without a
window. Tests: the default style returns the exact `Corners`, padding and
margin the arms built before, for active and inactive tabs at the top and the
bottom (R5); `Pill` returns half-height corners; `Slant` returns full-height
opposite corners; each marker's split keeps the border box (C2). Checkpoint:
auto, `cargo test -p wezterm-gui --bin wezterm-gui fancy_tab_bar`.

**U3 Docs.** R1–R4. Files: `docs/config/lua/config/fancy_tab_bar_style.md`,
`docs/changelog.md`. Test expectation: none, documentation.

**U4 Join `build`.** Depends on U2, U3. `git merge --no-ff` into `build`; the
conflicts are with `unit/tab-bar-rows`, `unit/tab-bar-row-status` and
`unit/tab-bar-fit` in `fancy_tab_bar.rs` and `tab_bar.rs` and are resolved in
the merge commit. Then a `PATCHES.md` row. Verify at contact: whether
`tab-bar-fit` measures a tab through the same element the bar draws; if it
measures the unwrapped element, the marker and the gap must be counted there
too. Checkpoint: auto, the U2 tests and the five `fancy_tab_bar` fit tests on
`build`.

**U5 Presets.** R6. Depends on C1 only. A bootstrap worktree, not the live
checkout: `configs/wezterm/tab_themes.lua` (the four presets as override
tables, and the cycle), its spec beside `tests/wezterm-config/tab_bar_spec.lua`,
and the key binding in `configs/wezterm.lua`. Checkpoint: auto, the spec.

**U6 Look.** Depends on U4, U5. A throwaway GUI built from `build`, each preset
applied. Checkpoint: the owner's eyes, because this process may not capture
the screen.

## Risks

- `tab-bar-fit` may measure tabs apart from the drawn elements (U4).
- A negative gap overlaps hit regions; a click in the overlap picks the tab
  drawn later.
- `Slant` and `Pill` corners need the tab's height in pixels, computed from the
  title font's cell height, the padding and the 1 px border; a fallback glyph
  taller than the cell would leave the corners short.
- Going live is two approvals: installing the engine, and merging the presets
  into bootstrap's live `configs/wezterm.lua`, where another session has
  uncommitted edits as of 2026-10-12 05:00.
