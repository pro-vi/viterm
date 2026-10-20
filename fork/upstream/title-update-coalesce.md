# gui: share one tab bar update among title notifications in a frame

One title written by a pane with OSC 0 reaches the window as three
notifications: `Alert::WindowTitleChanged` and `Alert::IconTitleChanged` from
the terminal, and `MuxNotification::WindowTitleChanged`, because
`LocalPaneNotifHandler` also sets the mux window's title from it. The window
answered each with `update_title()`, which rebuilds every tab through
`format-tab-title` when a handler is registered, so every title write cost
three full tab bar rebuilds. A program that updates its title several times a
second (a spinner, a progress indicator) then keeps the GUI thread busy in
proportion to the number of tabs.

Every notification in that match arm (the three above, and
`OutputSinceFocusLost`, `CurrentWorkingDirectoryChanged`, `TabTitleChanged` and
`Progress`) now schedules one update for the next frame (`1000 / max_fps` ms),
and any that arrive before it runs share it. Over a mux
connection the three arrive in separate turns of the event loop (the window
title change takes one hop to the main thread, a pane's alert two), so
deferring to the next turn alone would still leave two rebuilds. A rebuilt tab
bar is only seen at the next paint, so waiting a frame shows nothing later than
before. `update_title()` called from user actions (switching or moving tabs,
key tables, overlays, focus) is unchanged and rebuilds at once.

Validation on the unit branch, debug build:

A throwaway GUI with its own config: 11 tabs, a `format-tab-title` handler that
counts its calls, and one pane writing 40 OSC 0 titles one second apart between
two user-var markers. One rebuild calls the handler twice per tab.

| | before | after |
|---|---|---|
| local window: handler calls, rebuilds | 2,662, 121 | 902, 41 |
| over a unix domain, server built from the same commit | 2,662, 121 | 902, 41 |
| GUI CPU seconds between the markers, local / unix domain (range over two or three runs) | 1.3–1.9 / 1.7–2.5 | 0.7–0.8 / 0.7–1.1 |

The extra rebuild in each row is the start marker's own. So a title write cost
three rebuilds before and one after, in both topologies.

Deferring to the next turn of the event loop instead of the next frame gave 81
rebuilds over the unix domain, the window title change landing after the
deferred update had run. `cargo fmt --all -- --check` passes, and
`cargo test -p wezterm-gui` passes its 18 tests.

There is no unit test: `TermWindow` cannot be built without a window and a GPU
context, so the change was measured as above instead.

An AI coding agent wrote the change and ran the measurements. Nothing was
tested by hand. No upstream submission has been made.
