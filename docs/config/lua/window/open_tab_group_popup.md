# window:open_tab_group_popup(tab_index)

{{since('nightly')}}

Opens the popup of the tab group (see [set_tab_groups](set_tab_groups.md))
that holds the tab at `tab_index`, with that tab marked, without activating
any tab. `tab_index` counts from 0, as [ActivateTab](../keyassignment/ActivateTab.md)
does.

Returns `true` when it opened a popup. It replaces a popup that another group
has open. Returns `false` when the tab at `tab_index` is in no group drawn as
one item, when there is no tab at that index, or when that tab's group already
has its popup open; it then closes any open popup, since the caller goes to
the tab. Returns `false` and changes nothing when another modal, such as the
command palette, is open.

A `tab_index` that is not a whole number of 0 or more is an error.

The popup is the one [toggle_tab_group_popup](toggle_tab_group_popup.md)
describes. The marked tab is drawn inside a ring in the
`quick_select_label_bg` color.

`false` means no popup was opened, so a key binding can go to the tab
instead. Here, a number for a tab inside a folded group shows its group first,
and the same number again goes to it:

```lua
local function show_or_go(n)
  return wezterm.action_callback(function(window, pane)
    if not window:open_tab_group_popup(n) then
      window:perform_action(wezterm.action.ActivateTab(n), pane)
    end
  end)
end

config.keys = {
  { key = '1', mods = 'CMD', action = show_or_go(0) },
  { key = '2', mods = 'CMD', action = show_or_go(1) },
}
```
