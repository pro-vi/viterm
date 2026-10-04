---
tags:
  - keys
---
# `key_assignment_log`

{{since('nightly')}}

When set to a file path, each key press that performs a key assignment
appends one line of JSON to that file. The line names the binding, not
the text: ordinary typing that matches no key assignment writes nothing.
The directory is created if it does not exist.

On macOS, a key assignment that also appears in the menu bar is
activated through the menu when its key is pressed; that press is
logged too. Choosing the same menu item with the mouse is not.

```lua
config.key_assignment_log = wezterm.home_dir .. '/.local/state/wezterm/keys.jsonl'
```

Pressing `ALT-SHIFT-h` bound to `ActivatePaneDirection` appends:

```json
{"action":"act.ActivatePaneDirection 'Left'","key":"H","mods":"ALT","table":null,"time":1700000000.25}
```

* `time` is seconds since the Unix epoch.
* `key` and `mods` are the binding that matched, in the form
  `wezterm show-keys --lua` prints. As in that listing, `SHIFT` with a
  letter is folded into an uppercase key.
* `table` is the name of the key table that held the binding, such as
  `copy_mode`, or `null` for the default key table.
* `action` is the assignment, spelled as `wezterm show-keys --lua`
  spells it. An action created with `wezterm.action_callback` appears as
  the `EmitEvent` it is made of.

The line is written after the assignment has run, so the log does not
delay it. The file stays open while the path stays the same.
