# `window:set_left_status(string)`

{{since('20220807-113146-c2fee766')}}

This method can be used to change the content that is displayed in the tab bar,
to the left of the tabs.  The content is displayed
left-aligned and will take as much space as needed to display the content
that you set; it will not be implicitly clipped.

The parameter is a string that can contain escape sequences that change
presentation.

It is recommended that you use [wezterm.format](../wezterm/format.md) to
compose the string.

See [window:set_right_status](set_right_status.md) for examples.

{{since('nightly')}}

An optional second parameter selects which row of the tab bar the status
belongs to, counting from 1. It is useful together with
[tab_bar_rows](../config/tab_bar_rows.md), which can make the tab bar more than
one row tall:

```lua
-- the window wide summary goes on the second row, leaving the first to the
-- tabs and to whatever this pane's directory has to say
window:set_left_status(status, 2)
```

Omitting it means the first row. The retro tab bar draws a single line and
shows the first row's status only.
