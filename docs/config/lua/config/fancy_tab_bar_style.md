---
tags:
  - tab_bar
---
# `fancy_tab_bar_style`

{{since('nightly')}}

Sets the shape, spacing and active-tab marker of the tabs in the fancy tab
bar ([use_fancy_tab_bar](use_fancy_tab_bar.md)). Colors still come from the
[tab bar colors](../../appearance.md#tab-bar-appearance-colors) and the font
from [window_frame](window_frame.md). Every field is optional; with none set
the tab bar looks as it always has.

```lua
config.fancy_tab_bar_style = {
  tab_shape = 'Square',
  row_height = 1.4,
  active_tab_marker = 'TopLine',
  active_tab_marker_color = '#e11d48',
}
```

| Field | Values | Default |
|---|---|---|
| `tab_shape` | `"Rounded"`, `"Square"`, `"Pill"`, `"Slant"` | `"Rounded"` |
| `tab_corner_size` | dimension | `"0.5cell"` |
| `row_height` | number | `1.75` |
| `tab_padding_x` | dimension | `"0.5cell"` |
| `tab_padding_y` | dimension | `"0.25cell"` |
| `tab_gap` | dimension | `0` |
| `tab_vertical_align` | `"TerminalEdge"`, `"Middle"` | `"TerminalEdge"` |
| `active_tab_marker` | `"Fill"`, `"TopLine"`, `"BottomLine"`, `"Ring"` | `"Fill"` |
| `active_tab_marker_color` | color | the `active_tab` foreground |
| `active_tab_marker_width` | dimension | `"2px"` |
| `title_background_paints` | `"Fill"`, `"Text"` | `"Fill"` |
| `active_tab_title_background` | boolean | `true` |

A dimension is a number of pixels or a string with a unit: `"2px"`, `"1pt"`,
`"0.5cell"`. A `cell` here is the cell of the tab bar font, not the terminal
font.

* `tab_shape`: `Rounded` is square on the edge next to the terminal and
  rounded on the far edge. `Pill` rounds both ends fully. `Slant` draws a
  parallelogram leaning right.
* `tab_corner_size`: the corner radius of `Rounded`, and how far `Slant`
  leans.
* `row_height`: the height of the tab bar, as a multiple of the cell height of
  the tab bar font. The status areas take the same height.
* `tab_padding_x`, `tab_padding_y`: the space between a tab's edge and its
  title.
* `tab_gap`: the space between two tabs. A negative value overlaps them,
  which lets `Slant` tabs interlock: a gap of minus `tab_corner_size` makes
  their edges meet.
* `tab_vertical_align`: `TerminalEdge` puts a tab against the edge next to the
  terminal, with a small gap on the far edge. `Middle` centers it in the row.
* `active_tab_marker`: `Fill` marks the active tab with its colors only.
  `TopLine` and `BottomLine` draw a line across that edge of the active tab,
  and `Ring` draws a ring around it, following its shape. The marked tab stays
  the size of the others; the line or ring takes the space from its padding.
* `title_background_paints`: a
  [format-tab-title](../window-events/format-tab-title.md) handler can set a
  background color on a tab's title. `Fill` paints the tab with it. `Text`
  colors the title's text with it instead and leaves the tab its own
  background, which keeps tabs that sit side by side, or one row above
  another, from running together into one block of color.
* `active_tab_title_background`: with `false`, the active tab ignores that
  color and keeps its `active_tab` colors, whichever way it would paint.

Because it is one table, a key can switch between looks with
[window:set_config_overrides](../window/set_config_overrides.md); the fields
a look leaves out fall back to their defaults:

```lua
local looks = {
  {},
  { tab_shape = 'Pill', tab_vertical_align = 'Middle', tab_gap = '4px' },
}

config.keys = {
  {
    key = 't',
    mods = 'ALT',
    action = wezterm.action_callback(function(window)
      local next = ((wezterm.GLOBAL.tab_look or 1) % #looks) + 1
      wezterm.GLOBAL.tab_look = next
      local overrides = window:get_config_overrides() or {}
      overrides.fancy_tab_bar_style = looks[next]
      window:set_config_overrides(overrides)
    end),
  },
}
```
