---
tags:
  - tab_bar
---
# `tab_bar_status_width = 0`

{{since('nightly')}}

When set to a number of characters, every row of the fancy tab bar keeps that
width free at its right for the row's right status, whatever the status says at
the moment. A character here is the average width of the lowercase letters and
digits in the tab bar font, so `30` holds about thirty characters of a branch
name; the font's cell width is not used, because for a proportional font it is
wider than most letters. The tabs are laid out beside it, so a status that changes length, such
as one that shows the current git branch, never moves a tab to another row.

```lua
config.tab_bar_rows = 2
config.tab_bar_status_width = 30
```

The status text sits against the right edge of that space. Text wider than the
space loses characters from its start, so the end of the status stays visible.
The default of `0` gives the right status the width of its text, as before.

Only the fancy tab bar honours this option.
