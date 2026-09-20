# `tab:get_index()`

{{since('nightly')}}

Returns the 0-based index of this tab within its containing window, or `nil`
if the tab is not currently in a window.

This is the same value as the `index` field of
[window:tabs_with_info()](../mux-window/tabs_with_info.md) and as
`tab_index` on [TabInformation](../TabInformation.md).

```lua
local mux = wezterm.mux
local tab, pane, window = mux.spawn_window {}
assert(tab:get_index() == 0)
```
