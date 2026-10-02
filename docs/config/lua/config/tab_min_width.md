---
tags:
  - tab_bar
---
# `tab_min_width = 0`

{{since('nightly')}}

When set to a number of cells, the fancy tab bar shows every tab title at its
natural width, so a short title makes a narrow tab, and cuts long titles only
as far as is needed for every tab to fit in the bar's
[tab_bar_rows](tab_bar_rows.md). A title is never cut below this many cells;
a cut title ends in an ellipsis. The default of `0` leaves the tab bar as it
was, with every tab capped at an even share of its row.

```lua
config.tab_bar_rows = 2
config.tab_min_width = 12
```

The new tab button follows the last tab when that row has room for it, and is
left out rather than given a row of its own.

[window:tab_bar_fit()](../window/tab_bar_fit.md) reports how the tabs were
fitted, including the fewest rows that would hold every tab at this width and
the fewest that would show every title in full, so that a key assignment or an
event handler can choose the number of rows.

Only the fancy tab bar honours this option.
