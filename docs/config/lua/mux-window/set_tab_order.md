## window:set_tab_order(TAB_IDS)

{{since('nightly')}}

Puts the tabs named by `TAB_IDS`, a list of tab ids, into that order. They
take the places those tabs hold now: a tab the list does not name keeps its
place, and an id that is not in this window is ignored. The active tab stays
active. Returns `true` when any tab moved.

A move is announced the way the `MoveTab` key assignment announces one, so
every client attached to the window, and a mux server that keeps tab order,
learns the new order. One call replaces as many `MoveTab` steps as there are
tabs to move, without activating any of them.

```lua
local mux_win = window:mux_window()
local ids = {}
for _, info in ipairs(mux_win:tabs_with_info()) do
  table.insert(ids, 1, info.tab:tab_id())
end
-- reverse the order of every tab in the window
mux_win:set_tab_order(ids)
```
