#!/usr/bin/env python3
"""Check one resize-harness run: pane sizes from the server and the GUI, and
the GUI's resize counters. Prints one line per check and exits 1 if any fails.

Usage: check.py <scenario> <run-dir>

Set CHECK_NO_ECHO=1 to fail when the client sent any Resize while applying a
server pane list (mux.client.send.Resize.in_resync above 0)."""
import collections
import json
import os
import re
import sys

scenario, run = sys.argv[1], sys.argv[2]
failures = []


def report(name, ok, detail):
    print("%s %-14s %s" % ("PASS" if ok else "FAIL", name, detail))
    if not ok:
        failures.append(name)


def load(name):
    try:
        with open(os.path.join(run, name)) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def tabs_in_order(panes):
    """Tabs of the window that holds the most tabs, in tab order; any other
    window is not resized by the script and is left out."""
    per_window = collections.Counter()
    for p in panes:
        per_window[p["window_id"]] += 1
    main = per_window.most_common(1)[0][0] if per_window else None
    order, tabs = [], collections.defaultdict(list)
    for p in (p for p in panes if p["window_id"] == main):
        if p["tab_id"] not in tabs:
            order.append(p["tab_id"])
        tabs[p["tab_id"]].append(p)
    return [sorted(tabs[t], key=lambda p: (p["top_row"], p["left_col"])) for t in order]


def extent(tab):
    """The size the tab occupies: the zoomed pane's size, else the bounding box."""
    zoomed = [p for p in tab if p.get("is_zoomed")]
    if zoomed:
        return zoomed[0]["size"]["cols"], zoomed[0]["size"]["rows"]
    return (max(p["left_col"] + p["size"]["cols"] for p in tab),
            max(p["top_row"] + p["size"]["rows"] for p in tab))


def sizes(tab):
    return [(p["size"]["cols"], p["size"]["rows"]) for p in tab]


before, server, gui = load("server-before.json"), load("server-after.json"), load("gui-after.json")
report("gui-answered", bool(gui), "the GUI answered cli list" if gui else "no answer within 10 s: GUI busy or gone")
if not server or not before:
    report("server", False, "missing server list")
    sys.exit(1)

s_tabs = tabs_in_order(server)
g_tabs = tabs_in_order(gui) if gui else []
if g_tabs:
    same = len(s_tabs) == len(g_tabs) and all(sizes(a) == sizes(b) for a, b in zip(s_tabs, g_tabs))
    report("gui=server", same, "GUI and server report the same pane sizes" if same else "pane sizes differ between GUI and server")

window = None
try:
    with open(os.path.join(run, "gui.stderr"), errors="replace") as f:
        for line in f:
            m = re.search(r"apply_dimensions computed size TerminalSize \{ rows: (\d+), cols: (\d+)", line)
            if m:
                window = (int(m.group(2)), int(m.group(1)))
except OSError:
    pass
if window is None:
    window = collections.Counter(extent(t) for t in s_tabs).most_common(1)[0][0]
    print("     window         no apply_dimensions trace; using the size most tabs share")
odd = [i for i, t in enumerate(s_tabs) if extent(t) != window]
report("one-size", not odd, "every tab is %dx%d" % window if not odd else
       ("window %dx%d; tabs at other sizes: " % window) + ", ".join("#%d %dx%d" % ((i,) + extent(s_tabs[i])) for i in odd[:6]))

b_tabs = tabs_in_order(before)
skewed = []
for i, (b, a) in enumerate(zip(b_tabs, s_tabs)):
    if len(b) == 2 and len(a) == 2 and not any(p.get("is_zoomed") for p in a + b):
        l0, r0 = b[0]["size"]["cols"], b[1]["size"]["cols"]
        l1, r1 = a[0]["size"]["cols"], a[1]["size"]["cols"]
        if abs(l1 - r1) > 1 + abs(l0 - r0):
            skewed.append("#%d %d|%d (was %d|%d)" % (i, l1, r1, l0, r0))
report("balance", not skewed, "splits keep their balance" if not skewed else "skewed: " + ", ".join(skewed[:6]))

if scenario == "zoom":
    stuck = [i for i, t in enumerate(s_tabs) if any(p.get("is_zoomed") for p in t) and extent(t) != window]
    report("zoom", not stuck, "zoomed panes fill the window" if not stuck else "zoomed panes off-size in tabs " + str(stuck))

if scenario == "cli-split":
    has = bool(g_tabs) and len(g_tabs[-1]) == 3
    report("cli-split", has, "the split made through the CLI shows in the GUI" if has else "the GUI does not show the CLI's split")

counters = {}
try:
    with open(os.path.join(run, "gui.stderr"), errors="replace") as f:
        for line in f:
            m = re.match(r"Key\((mux\.client\.[\w.]+|gui\.tabbar\.rebuild\.by\.mux\.TabResized)\)\s+(\d+)\s*$", line.strip())
            if m:
                counters[m.group(1)] = int(m.group(2))
except OSError:
    pass
print("     counters       " + (", ".join("%s=%d" % kv for kv in sorted(counters.items())) or "none printed"))
if os.environ.get("CHECK_NO_ECHO") == "1":
    echoes = counters.get("mux.client.send.Resize.in_resync", 0)
    report("no-echo", echoes == 0, "no Resize sent while applying a server pane list" if echoes == 0 else "%d Resize sent while applying a server pane list" % echoes)

sys.exit(1 if failures else 0)
