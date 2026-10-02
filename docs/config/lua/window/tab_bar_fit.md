# window:tab_bar_fit()

{{since('nightly')}}

Returns how the fancy tab bar last fitted its tabs into its rows, or `nil`
when [tab_min_width](../config/tab_min_width.md) is not set or the tab bar has
not been drawn yet. The value is a table with these fields:

* `rows` - the number of rows the tab bar has now
* `rows_min` - the fewest rows that hold every tab with long titles cut to
  `tab_min_width`, or `nil` when that would take more than 8 rows
* `rows_full` - the fewest rows that hold every tab with every title in full,
  or `nil` when that would take more than 8 rows
* `title_width` - the number of cells that long titles are cut to now
* `tabs_per_row` - an array with the number of tabs on each row, top row first

This example uses the fewest rows that hold every tab whenever the number of
tabs changes:

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.tab_min_width = 12
config.tab_bar_status_width = 30

local last_count = {}

wezterm.on('update-status', function(window, pane)
  local fit = window:tab_bar_fit()
  local count = #window:mux_window():tabs()
  local id = window:window_id()
  if not fit or not fit.rows_min or last_count[id] == count then
    return
  end
  last_count[id] = count
  if fit.rows ~= fit.rows_min then
    local overrides = window:get_config_overrides() or {}
    overrides.tab_bar_rows = fit.rows_min
    window:set_config_overrides(overrides)
  end
end)

return config
```
