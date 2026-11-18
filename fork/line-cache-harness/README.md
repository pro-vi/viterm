# Line-cache harness

Shows whether a mux-attached GUI keeps each pane's line cache within its
limit, against a throwaway mux server, without touching a real session. A
run starts a server from the config here (one private unix domain,
`scrollback_lines = 1000`) whose one pane prints numbered lines at a steady
rate, attaches a GUI, and reads the GUI's memory footprint before and after
the lines arrive.

```zsh
fork/line-cache-harness/run.zsh <scenario> <out-dir>/line-cache-harness/<name> \
  <path to wezterm-gui> <dir holding wezterm-mux-server and wezterm> [lines] [limit-mb]
```

| scenario | what it does |
|---|---|
| `steady` | attach, then print |
| `resize` | the same, with one window resize 3 s after attach |

The client sizes a pane's cache from `scrollback_lines`, so here it may hold
1,000 lines. The pane first prints 5,000 warm-up lines, which fill a cache
that keeps its limit; the first reading is taken after them. It then prints
the measured lines (100,000 by default, at up to 1,000 a second). A cache
that keeps its limit only replaces what it holds, so the GUI's footprint
barely moves, and the run fails when it grows by more than `limit-mb` (32 by
default). `result.txt` in the out-dir has the verdict and `footprint.csv` a
reading every two seconds.

Measured with debug builds of one tree, 100,000 lines:

| build | `steady` | `resize` |
|---|---|---|
| wezterm `cab251610` | grew 344.5 MB, FAIL | grew 354.1 MB, FAIL |
| the same with wezterm/wezterm#7704 | grew 5.9 MB, PASS | grew 4.0 MB, PASS |

`steady` fails without the fix too: a GUI resizes each pane once when it
attaches, and that resize already drops the limit. The growth is about
3.6 KB for each printed line, which are 112 columns wide with a colour change
every 8 cells.

The GUI window appears on screen while a run is going. The out-dir must not
exist yet, and `probe.lua` refuses any socket outside a `line-cache-harness`
directory. `footprint.py` reads the footprint through libproc, so the harness
runs on macOS only.
