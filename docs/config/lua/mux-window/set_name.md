## window:set_name(NAME)

{{since('nightly')}}

Names the window; an empty string removes its name. Unlike
[window:set_title()](set_title.md), which applications may change at any time
with title escape sequences, the name changes only when it is set again.

In a GUI attached to a multiplexer domain, the name is set on the
multiplexer server's window. `wezterm cli list --format json` reports each
window's name as `window_name`, empty for a window that has none.

```lua
window:set_name 'notes'
```
