# Resize harness

Reproduces the ways a mux-attached GUI loses track of pane sizes, against a
throwaway mux server, without touching a real session. Each run starts a
server from the config here (one private unix domain, panes running `sleep`),
makes a single-pane tab and N tabs split left|right through the server's CLI,
attaches a GUI, drives a scripted series of window resizes from Lua, then
compares the pane sizes the server and the GUI report.

```zsh
fork/resize-harness/run.zsh <scenario> /private/tmp/<you>/resize-harness/<name> \
  <path to wezterm-gui> <dir holding wezterm-mux-server and wezterm> [split-tabs]
```

| scenario | what it does | catches |
|---|---|---|
| `burst` | 20 resizes 40 ms apart, 3 s after attach | splits drifting off balance, tabs left at a burst size |
| `slow` | 6 resizes 2 s apart | the control: should always pass |
| `zoom` | `burst`, with the right pane zoomed in every other tab | zoomed panes keeping their zoom-time size |
| `attach` | `burst` starting 0.1 s after attach | the same, while the attach is still settling |
| `cli-split` | `burst`, then a split made through the server's CLI | a change made outside the GUI not reaching it |

`check.py` prints PASS or FAIL per check and exits 1 on any failure. The
window size is the GUI's own terminal size, read from the trace line
`apply_dimensions` writes (the runner enables that one log); without it, the
checker falls back to the size most tabs share. `CHECK_NO_ECHO=1` also fails the run if the GUI's stats show any
`mux.client.send.Resize.in_resync`, which needs a GUI carrying
`unit/client-resync-counters`. `PROBE_PAD_KEYS=2000` slows each tab bar
rebuild the way a large config does.

The GUI windows appear on screen while a run is going. The output directory
must contain `resize-harness`, and `probe.lua` refuses any socket outside such
a directory.

## The live ViTerm after an install

`live-check.py` reads the running ViTerm on `human-main` and changes nothing. Save the server's
pane list before the GUI is relaunched, then check after the relaunch and again after resizing
the window (fullscreen on and off is the burst that skewed splits):

```zsh
fork/resize-harness/live-check.py snapshot ~/.local/state/viterm/before-relaunch.json
fork/resize-harness/live-check.py check ~/.local/state/viterm/before-relaunch.json
```

It reports which engine the GUI and server run, panes whose size differs from the snapshot, GUI
against server, and the GUI's resize counters. A counter missing from the stats table is 0. The
GUI numbers its own panes and reports no `tty_name`, so it is matched to the server by list
order and title; the snapshot is matched by the server's pane ids, which survive a relaunch.
