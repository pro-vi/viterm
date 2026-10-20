# window:toggle_tab_group_popup()

{{since('nightly')}}

Opens the popup of the tab group (see [set_tab_groups](set_tab_groups.md))
that holds the active tab, or closes the popup when one is open. Returns
`false` when no popup was open and the active tab is in no group drawn as one
item, and `true` otherwise.

The popup is what clicking a group's item in the fancy tab bar opens: the
group's tabs, drawn as the tab bar draws tabs and with the titles they would
have there, in a box of the tab bar's color with the corners of the tab bar's
tab shape, next to the item and over the bottom (or top) lines of the panes,
with a point aimed at the item. The tab bar and the panes do not move.

Clicking one of its tabs activates that tab and closes the popup. Escape, a
click outside it, or a second click on the group's item closes it; a click in
the tab bar closes it and then does what it would have done. While it is open
it takes the keyboard as the command palette does: a key with a key assignment
still performs it, so `ActivateTab` reaches any tab, and other keys do nothing.
It closes by itself when its group stops being drawn as one item.

The popup is drawn only by the fancy tab bar (`use_fancy_tab_bar = true`).

```lua
config.keys = {
  {
    key = 'F',
    mods = 'SHIFT|ALT',
    action = wezterm.action_callback(function(window, pane)
      window:toggle_tab_group_popup()
    end),
  },
}
```
