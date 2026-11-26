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

## What the fix should do on a live window

The harness shows the mechanism on a throwaway server. This section is the
claim about a real session, written down before any live window has run the
fix, so that it can be checked later and found wrong.

**Before.** A window attached to a mux kept every line its panes sent it, so
its memory grew for as long as it stayed open. One window with about 145
panes, read from a recorder that samples each process's footprint once a
minute:

| days open | footprint |
|---|---|
| 3.5 | 4.7 GB |
| 7 | 8.0 GB |
| 10 | 11.0 GB |
| 13 | 14.6 GB |
| 14 | 24.2 GB |
| 15 | 43.6 GB |

That is 1.1 GB a day for the first two weeks, then 30 GB in 16 hours while
its panes printed more. Nearly all of it was swapped out. On day 15 the
machine ran out of memory while this window held 43 GB.

**After, what is claimed.**

1. The footprint has a ceiling: about 0.5 GB plus panes × `scrollback_lines`
   × the bytes a cached line takes (3.6 KB for the harness's lines). With 148
   panes and `scrollback_lines = 50000` that is 25 GB. A window over its
   ceiling falsifies the fix.
2. The shape changes from a line that keeps rising to one that levels off.
   A pane stops adding memory once it has printed `scrollback_lines` lines, so
   the panes that print most, which drove the growth, stop first. After two
   weeks the footprint should rise much more slowly than the 1.1 GB a day
   above. A window that still gains 1 GB a day in its third week is failing
   the claim even while it is under the ceiling.
3. Nothing a person sees in a pane changes. The lines dropped from the cache
   are ones the server no longer has either, or ones fetched again when
   scrolled to.

**Not claimed.** The ceiling is not small: the fix restores the limit, it
does not lower it. The mux server's memory is untouched. Tab bar and input
latency are untouched. A machine can still run out of memory when another
program takes it all. On an engine without `reload-drops-old-backgrounds`
(in `PATCHES.md`), background gradient images also grow the window, by about
60 MB for each config reload that changes the palette; they are
`MALLOC_LARGE` regions the size of the window in `footprint --pid <pid>`, and
do not count for or against this fix.

**How to check.** `live-check.py` reads the running window and changes
nothing:

```zsh
fork/line-cache-harness/live-check.py --scrollback-lines 50000
```

It prints the engine each window runs, days open, footprint, the ceiling for
the server's pane count, and the change the recorder saw, then one verdict a
window: `NOT FIXED` (the engine predates the fix), `TOO EARLY` (open for less
than 14 days), `FAIL` (over the ceiling) or `PASS`. Run against the window in
the table, it reports `NOT FIXED`; told to judge that window as if it had the
fix (`--fixed-from 20261101-000000`), it reports `FAIL`, 45.5 GB against a
ceiling of 25.3 GB.
