# `wezterm cli set-window-name NAME`

{{since('nightly')}}

*Run `wezterm cli set-window-name --help` to see more help*

Names a window; an empty `NAME` removes its name. Unlike the window's title,
which a program in any of its panes may change with a title escape sequence,
the name changes only when it is set again. `wezterm cli list --format json`
reports it as `window_name`.

## Synopsis

```console
{% include "../../examples/cmd-synopsis-wezterm-cli-set-window-name--help.txt" %}
```
