---
tags:
  - tab_bar
---
# `tab_bar_rows = 1`

{{since('nightly')}}

Sets how many rows of tabs the tab bar shows. With the default of `1` the tab
bar is a single row, as it has always been. With a larger value the tab bar is
that many rows tall and the tabs are spread over the rows in order, which lets
each tab be wider and show more of its title:

```lua
config.tab_bar_rows = 2
```

The width available to a tab is divided by the number of tabs on one row rather
than by the total, so two rows roughly doubles how much of each title is
visible. The left and right status areas stay on the first row.

Because this is an ordinary config value you can change it at runtime for a
single window with
[window:set_config_overrides()](../window/set_config_overrides.md), for example
to toggle between one and two rows from a key assignment:

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.keys = {
  {
    key = 'r',
    mods = 'ALT',
    action = wezterm.action_callback(function(window)
      local overrides = window:get_config_overrides() or {}
      overrides.tab_bar_rows = (overrides.tab_bar_rows == 2) and 1 or 2
      window:set_config_overrides(overrides)
    end),
  },
}

return config
```

Only the fancy tab bar honours this option. The retro tab bar
([use_fancy_tab_bar = false](use_fancy_tab_bar.md)) draws a single terminal
line and is always one row.
