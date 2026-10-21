# window:set_tab_groups(GROUPS)

{{since('nightly')}}

Asks the fancy tab bar to draw runs of adjacent tabs as one item each.
`GROUPS` is a list of tables, each with:

* `tabs` - a list of tab ids
* `label` - the text the group's item shows after its tab numbers
* `marks` - optional: tabs of the group to name after the label, each a table
  `{ tab = tab_id, glyph = text }`. The item shows each as that tab's number
  followed by its glyph, in the order given, such as `7!` for a tab whose
  program is waiting for an answer.
* `busy` - optional: text the item shows after its marks.

A mark naming a tab that is not in `tabs`, or with no glyph, is an error.

A group is drawn as one item when two or more of its tabs are in this window
and they sit next to each other in its tab order; otherwise its tabs are drawn
as they always are. A tab named by two groups goes to the first of them that
is drawn as one item, and an id that is not in the window is ignored. With the
retro tab bar, or with the tab bar hidden, every tab is drawn as itself. The
item reads the numbers of its first and last tabs, numbered as the tab bar
numbers tabs (see
[show_tab_index_in_tab_bar](../config/show_tab_index_in_tab_bar.md) and
[tab_and_split_indices_are_zero_based](../config/tab_and_split_indices_are_zero_based.md)),
then the label, the marks and the busy text. Marks are numbered as the tab
bar numbers tabs too, whatever a `format-tab-title` handler writes in the
tabs' own titles. When the item has less room than its text needs, the label
is shortened first, down to one character and an ellipsis; then the busy text
is left out; then marks are left out from the end, and `+n` says how many.
Its tab numbers, and any mark it shows, are never cut. It is drawn as a tab
with layers showing past its trailing
edge, and the way the active tab is drawn when it holds the active tab.
Clicking it opens a popup over the panes, next to the item, that lists the
group's tabs drawn as the tab bar draws tabs; see
[toggle_tab_group_popup](toggle_tab_group_popup.md).

`format-tab-title` is still called for every tab, including the tabs a group
holds, and the tab numbers do not change: key assignments such as
`ActivateTab` reach every tab as before.

Each call replaces the groups set before; an empty list removes them all. The
groups are kept for this window only, until the window closes. Calling it again
with the same groups does not redraw the tab bar.

```lua
wezterm.on('update-status', function(window)
  local ids = {}
  for _, info in ipairs(window:mux_window():tabs_with_info()) do
    table.insert(ids, info.tab:tab_id())
  end
  -- show the first three tabs as one item
  window:set_tab_groups { { tabs = { ids[1], ids[2], ids[3] }, label = 'work' } }
end)
```
