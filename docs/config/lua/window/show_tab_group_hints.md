# window:show_tab_group_hints()

{{since('nightly')}}

Draws a hint on each tab group that the fancy tab bar shows as one item (see
[set_tab_groups](set_tab_groups.md)), so that one key opens a group's popup
without activating any tab.

Each folded group's item reads its hint where the numbers of its first and
last tabs were, and keeps its width, so nothing in the tab bar moves. Every
other tab is dimmed. With tab numbers turned off
([show_tab_index_in_tab_bar](../config/show_tab_index_in_tab_bar.md)) there
are no numbers to replace, so the hint is added in front of the label and the
item grows by it. The hints are letters from
[quick_select_alphabet](../config/quick_select_alphabet.md), one per folded
group in the order they are drawn, in the `quick_select_label_fg` and
`quick_select_label_bg` colors. A group past the end of the alphabet keeps its
numbers and gets no hint.

While the hints are shown:

* a group's hint opens that group's popup (see
  [toggle_tab_group_popup](toggle_tab_group_popup.md)) and the active tab stays
  as it is;
* Escape closes the hints;
* a key with Cmd, Ctrl or Alt still performs its key assignment, and the hints
  stay;
* a key with Cmd, Ctrl or Alt that has no key assignment closes the hints and
  goes on to the pane;
* any other key closes the hints and goes on as it would with no hints: to its
  key assignment if it has one, else to the pane;
* an overlay opening over the active tab, such as quick select, closes them;
* a mouse press closes the hints and then does what it would have done;
* activating another tab closes them.

Returns `true` when it drew the hints, replacing an open tab group popup.
Returns `false`, changing nothing, when the hints are already shown, when no
group is drawn as one item (the retro tab bar draws none), when the alphabet
gives no hint, or when another modal, such as the command palette, is open.

The `false` from a second press makes one key do both: the first press shows
the hints, and the second opens the popup of the active tab's own group.

```lua
config.keys = {
  {
    key = 'F',
    mods = 'SHIFT|ALT',
    action = wezterm.action_callback(function(window, pane)
      if not window:show_tab_group_hints() then
        window:toggle_tab_group_popup()
      end
    end),
  },
}
```
