#!/usr/bin/env python3
"""Check the live ViTerm on `human-main` after an engine install, without touching it.

    live-check.py snapshot OUT.json   save the server's pane list before the GUI is relaunched
    live-check.py check SNAPSHOT.json after the relaunch, and again after resizing the window

`check` reports which engine the GUI and server run, whether each pane still has the size the
snapshot recorded, whether the GUI and the server agree, and the GUI's own resize counters.

The server's pane ids are stable across a GUI relaunch, so the snapshot is compared by them.
The GUI numbers its panes itself (1..N in attach order) and reports no `tty_name`, so the GUI
list is matched to the server's by list order and title, never by pane id.
"""
import collections, json, os, re, subprocess, sys, time

HOME = os.path.expanduser("~")
ENGINE = f"{HOME}/.local/opt/viterm-engine.app/Contents/MacOS"
SOCKET = f"{HOME}/.local/share/wezterm/human-main.sock"
LOG = f"{HOME}/.local/state/viterm/gui-stderr.log"
COUNTERS = ["mux.client.send.Resize.in_resync", "mux.client.send.Resize", "mux.client.resync",
            "mux.client.recv.TabResized", "gui.tabbar.rebuild.by.Resized",
            "gui.tabbar.rebuild.by.mux.TabResized"]


def cli_list(sock):
    env = dict(os.environ, WEZTERM_UNIX_SOCKET=sock)
    out = subprocess.run(
        ["perl", "-e", 'alarm shift; exec @ARGV or die "cannot run $ARGV[0]: $!\\n"', "15",
         f"{ENGINE}/wezterm", "cli", "--no-auto-start", "list", "--format", "json"],
        capture_output=True, text=True, env=env).stdout
    try:
        return json.loads(out)
    except ValueError:
        return None


def engine_processes():
    rows = []
    for line in subprocess.run(["ps", "-axo", "pid,lstart,command"], capture_output=True, text=True).stdout.splitlines():
        m = re.match(r"\s*(\d+)\s+(\w+ \w+\s+\d+ [\d:]+ \d+)\s+(\S+)", line)
        if m and m.group(3).startswith(ENGINE) and m.group(3).rsplit("/", 1)[1] in ("wezterm-gui", "wezterm-mux-server"):
            started = time.mktime(time.strptime(re.sub(r"\s+", " ", m.group(2)), "%a %b %d %H:%M:%S %Y"))
            rows.append((int(m.group(1)), m.group(3).rsplit("/", 1)[1], started))
    return rows


def size(pane):
    return (pane["size"]["cols"], pane["size"]["rows"], pane["is_zoomed"])


def snapshot(out):
    panes = cli_list(SOCKET)
    if panes is None:
        sys.exit("the server on human-main did not answer")
    json.dump(panes, open(out, "w"))
    print(f"saved {len(panes)} panes in {len({p['tab_id'] for p in panes})} tabs to {out}")


def check(path):
    installed = os.path.getmtime(f"{ENGINE}/wezterm-gui")
    print(f"== engine installed {time.strftime('%m-%d %H:%M:%S', time.localtime(installed))}")
    gui_pid = None
    for pid, kind, started in engine_processes():
        print(f"  {kind} pid {pid} started {time.strftime('%m-%d %H:%M:%S', time.localtime(started))}, "
              f"{'after' if started > installed else 'BEFORE'} the install")
        if kind == "wezterm-gui":
            gui_pid = pid

    snap = {p["pane_id"]: p for p in json.load(open(path))}
    server = cli_list(SOCKET)
    print("== server against the snapshot (matched by pane id)")
    if server is None:
        print("  no answer")
    else:
        now = {p["pane_id"]: p for p in server}
        common = set(snap) & set(now)
        diff = sorted(i for i in common if size(snap[i]) != size(now[i]))
        print(f"  {len(now)} panes, {len(common)} in the snapshot, {len(common) - len(diff)} with the same size and zoom, "
              f"{len(diff)} different; only in the snapshot {sorted(set(snap) - set(now))}, only now {sorted(set(now) - set(snap))}")
        for i in diff[:12]:
            print(f"    pane {i}: was {size(snap[i])}, now {size(now[i])}")

    print("== GUI against the server (matched by list order and title)")
    gui = cli_list(f"{HOME}/.local/share/wezterm/gui-sock-{gui_pid}") if gui_pid else None
    if server is None or gui is None:
        print("  no answer")
    else:
        key = lambda p: (p["title"], p["left_col"], p["top_row"])
        pairs = list(zip(server, gui))
        aligned = [(s, g) for s, g in pairs if key(s) == key(g)]
        bad = [(s, g) for s, g in aligned if size(s) != size(g)]
        print(f"  {len(server)} server panes, {len(gui)} GUI panes, {len(aligned)} aligned, {len(bad)} with a different size")
        for s, g in bad[:8]:
            print(f"    {s['title'][:24]}: server {size(s)}, GUI {size(g)}")

    print("== split pairs now, left|right, by how many tabs")
    tabs = collections.defaultdict(list)
    for p in (server or []):
        tabs[p["tab_id"]].append(p)
    pairs = collections.Counter()
    for panes in tabs.values():
        if len(panes) == 2 and panes[0]["top_row"] == panes[1]["top_row"] and not any(p["is_zoomed"] for p in panes):
            a, b = sorted(panes, key=lambda p: p["left_col"])
            pairs[(a["size"]["cols"], b["size"]["cols"])] += 1
    print(" ", dict(sorted(pairs.items(), key=lambda kv: -kv[1])))

    print("== GUI counters, the last complete stats table in the log (written every 60 s by periodic_stat_logging)")
    text = open(LOG, "rb").read()[-400000:].decode("utf8", "replace")
    # A counter is listed only once it has been incremented, so one that is missing is 0. The tables
    # list counters alphabetically, so a table that has reached `rpc.count` is complete for these;
    # reading further back would pick up values from an earlier GUI process.
    complete = [t for t in text.split("STAT                ")[1:] if "\nKey(rpc.count" in t]
    last = {}
    if complete:
        for line in complete[-1].splitlines():
            m = re.match(r"Key\(([^)]*)\)\s+(\d+)\s*$", line)
            if m:
                last[m.group(1)] = int(m.group(2))
    for name in COUNTERS:
        print(f"  {name:44s} {last.get(name, 0)}{'' if name in last else '  (not in the table)'}")
    errors = [l for l in text.splitlines() if re.search(r"\b(ERROR|panicked)\b", l) and "UNErrorDomain" not in l]
    print(f"  ERROR or panic lines in the recent log (toast notices excluded): {len(errors)}")
    for l in errors[-3:]:
        print("   ", l[:180])


if len(sys.argv) == 3 and sys.argv[1] == "snapshot":
    snapshot(sys.argv[2])
elif len(sys.argv) == 3 and sys.argv[1] == "check":
    check(sys.argv[2])
else:
    sys.exit(__doc__)
