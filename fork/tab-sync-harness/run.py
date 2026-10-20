#!/usr/bin/env python3
"""Tab sync harness: does a mux-attached GUI show the tabs the server has?

Starts a throwaway mux server from probe.lua, runs a scenario with real GUIs
attached to it, and compares the tabs each GUI lists with the server's.

    fork/tab-sync-harness/run.py <scenario> <out-dir> <gui-binary> <server-dir>

scenario:
  ghost  a tab is spawned in the second window and its only pane is moved
         into a tab of the first (`split-pane --move-pane-id`), so the server
         removes the emptied tab. A server that announces the move makes the
         GUI resync at once, and the checks marked "after the move" need that
         resync: the moved pane once, the emptied tab gone. The same checks
         run again after a spawn, which makes every client resync whatever
         the server announces.
  order  one window of four titled tabs and two GUIs; the second GUI moves
         the first tab to the third place, as the MoveTab key does. The
         server must take the new order, the first GUI must show it, and a
         GUI attached afterwards must show it too. A tab spawned after the
         move makes every client resync, and the moving GUI must still show
         its move after that, against any server. WATCHER_GUI=<path> runs
         the first GUI from another binary; one that predates tab orders
         fails the checks of its order by design, and its "still attached"
         and "no lost connection" checks are the ones that count.

out-dir must be new or empty, with a folder whose name starts with
"tab-sync-harness" in its path, as probe.lua asserts. The server's socket and
its HOME live in it, so not even a config that fails to load reaches the
socket of a live session.
server-dir holds the wezterm-mux-server and wezterm (CLI) binaries to use.
The GUI window appears on screen while a run is going. Prints PASS or FAIL
per check and exits 1 on any failure. The checks after a split, a move or a
spawn read the GUIs once after a fixed wait, since some of them assert that a
move is not undone: in ghost 3 s after the split and after the spawn; in order
about 3 s after the move (the harness waits 6 s once the moving GUI attaches,
and that GUI moves 3 s after attaching) and 2 s after the moving GUI lists the
spawned tab. On a loaded machine a late answer therefore reads as FAIL. Each
GUI's log shows the move with its time; WEZTERM_LOG=wezterm_client::domain=debug
adds a line for each pane list a GUI applies.
"""
import collections, json, os, pathlib, re, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
CONFIG = os.path.join(HERE, "probe.lua")


def main():
    if len(sys.argv) != 5:
        sys.exit(__doc__)
    scenario, run, gui_bin, srv_dir = sys.argv[1:]
    run = os.path.abspath(run)
    if not re.search(r"/tab-sync-harness[^/]*/", os.path.join(run, "sock")):
        sys.exit("out-dir needs a folder whose name starts with tab-sync-harness in its path")
    if os.path.isdir(run) and os.listdir(run):
        sys.exit(f"out-dir {run} is not empty; give a new one")
    if scenario not in SCENARIOS:
        sys.exit(f"unknown scenario {scenario}")
    os.makedirs(run, exist_ok=True)
    h = Harness(run, gui_bin, srv_dir)
    try:
        failures = SCENARIOS[scenario](h)
    finally:
        h.stop()
    print(f"{scenario}: {'PASS' if not failures else 'FAIL ' + ', '.join(failures)}")
    sys.exit(1 if failures else 0)


class Harness:
    def __init__(self, run, gui_bin, srv_dir):
        self.run, self.gui_bin, self.srv_dir = run, gui_bin, srv_dir
        self.sock = os.path.join(run, "sock")
        self.env = {k: v for k, v in os.environ.items()
                    if k not in ("WEZTERM_UNIX_SOCKET", "WEZTERM_PANE", "WEZTERM_CONFIG_FILE")}
        self.env["PROBE_SOCK"] = self.sock
        self.failures = []
        self.guis = []
        self.started = 0
        home = os.path.join(run, "home")
        os.makedirs(home)
        self.server = subprocess.Popen(
            [os.path.join(srv_dir, "wezterm-mux-server"), "--config-file", CONFIG],
            stdout=open(os.path.join(run, "server.log"), "w"), stderr=subprocess.STDOUT,
            env={**self.env, "HOME": home})
        try:
            for _ in range(100):
                if os.path.exists(self.sock):
                    break
                time.sleep(0.1)
            else:
                raise RuntimeError(f"the server (pid {self.server.pid}) made no socket in 10 s; stopped it")
        except BaseException:
            self.stop()
            raise

    def cli(self, *args, sock=None):
        env = {**self.env, "WEZTERM_UNIX_SOCKET": sock or self.sock}
        r = subprocess.run([os.path.join(self.srv_dir, "wezterm"), "--config-file", CONFIG,
                            "cli", "--no-auto-start", *args],
                           env=env, capture_output=True, text=True, timeout=10)
        if r.returncode != 0:
            raise RuntimeError(f"cli {' '.join(args)}: {r.stderr.strip()[:300]}")
        return r.stdout.strip()

    def server_rows(self):
        return json.loads(self.cli("list", "--format", "json"))

    def attach(self, extra_env=None, gui_bin=None):
        """Start a GUI; the latest one is what gui_rows and wait_for_gui_tabs
        read unless they are given another."""
        # Named by how many GUIs this run started, so a GUI attached after
        # others detached does not overwrite their logs.
        log = open(os.path.join(self.run, f"gui-{self.started}.log"), "w")
        self.started += 1
        gui = subprocess.Popen(
            [gui_bin or self.gui_bin, "--config-file", CONFIG, "connect", "probe"],
            stdout=log, stderr=subprocess.STDOUT, env={**self.env, **(extra_env or {})})
        gui.sock = os.path.expanduser(f"~/.local/share/wezterm/gui-sock-{gui.pid}")
        gui.log = log.name
        self.guis.append(gui)
        return gui

    def detach(self, gui):
        self.end(gui)
        self.guis.remove(gui)

    def gui_rows(self, gui=None):
        gui = gui or self.guis[-1]
        return json.loads(self.cli("list", "--format", "json", sock=gui.sock))

    def wait_for_gui_tabs(self, n, gui=None, timeout=20):
        """Wait until the GUI lists n tabs; True when it did."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                if len(tabs(self.gui_rows(gui))) == n:
                    return True
            except (RuntimeError, json.JSONDecodeError, subprocess.TimeoutExpired):
                pass
            time.sleep(0.25)
        return False

    def check(self, name, ok, detail=""):
        print(f"{'PASS' if ok else 'FAIL'}  {name}" + (f"  ({detail})" if detail else ""))
        if not ok:
            self.failures.append(name)

    def save(self, name, rows):
        with open(os.path.join(self.run, name + ".json"), "w") as f:
            json.dump(rows, f, indent=1)

    def stop(self):
        for gui in self.guis:
            self.end(gui)
        self.end(self.server)

    def end(self, proc):
        """End a GUI or the server, killing it if it outlives 5 s. A GUI's
        socket file goes too: one ended by a signal leaves it behind."""
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
        if proc is not self.server:
            pathlib.Path(proc.sock).unlink(missing_ok=True)


def tabs(rows):
    """Each tab as (window id, tab id, sorted pane ids), in list order."""
    seen = {}
    for r in rows:
        seen.setdefault((r["window_id"], r["tab_id"]), []).append(r["pane_id"])
    return [(w, t, sorted(p)) for (w, t), p in seen.items()]


def panes_in_two_tabs(rows):
    where = {}
    for r in rows:
        where.setdefault(r["pane_id"], set()).add(r["tab_id"])
    return {p: sorted(t) for p, t in where.items() if len(t) > 1}


def tabs_per_window(rows):
    return sorted(collections.Counter(w for w, _t, _p in tabs(rows)).values())


def first_pane(h):
    """The server's first pane, which the server spawns itself shortly after
    its socket appears."""
    deadline = time.monotonic() + 10
    rows = h.server_rows()
    while not rows and time.monotonic() < deadline:
        time.sleep(0.1)
        rows = h.server_rows()
    return rows[0]


def same_as_server(h, when, server, gui):
    """The GUI shows the server's tabs: as many, none holding a pane twice,
    and each window holding as many as its server window."""
    h.check(f"{when}: the GUI lists as many tabs as the server",
            len(tabs(gui)) == len(tabs(server)), f"GUI {len(tabs(gui))}, server {len(tabs(server))}")
    h.check(f"{when}: the GUI shows no pane in two tabs", not panes_in_two_tabs(gui),
            str(panes_in_two_tabs(gui)))
    h.check(f"{when}: the GUI's windows hold the server's tab counts",
            tabs_per_window(gui) == tabs_per_window(server),
            f"GUI {tabs_per_window(gui)}, server {tabs_per_window(server)}")


def ghost(h):
    first = first_pane(h)
    w1, a = first["window_id"], first["pane_id"]
    h.cli("spawn", "--window-id", str(w1))
    c = int(h.cli("spawn", "--new-window"))
    w2 = next(r["window_id"] for r in h.server_rows() if r["pane_id"] == c)
    h.cli("spawn", "--window-id", str(w2))
    h.attach()
    h.check("GUI attaches and shows the server's 4 tabs", h.wait_for_gui_tabs(4))

    x = int(h.cli("spawn", "--window-id", str(w2)))
    h.check("GUI shows the spawned tab", h.wait_for_gui_tabs(5))
    h.cli("split-pane", "--pane-id", str(a), "--right", "--move-pane-id", str(x))
    time.sleep(3)

    server, gui = h.server_rows(), h.gui_rows()
    h.save("server-after-move", server)
    h.save("gui-after-move", gui)
    moved = next(r for r in server if r["pane_id"] == x)
    h.check("server: the moved pane is in the first tab, the emptied tab is gone",
            moved["tab_id"] == first["tab_id"] and len(tabs(server)) == 4,
            f"{len(tabs(server))} tabs")
    # The server announces the move only if a split notifies its clients;
    # without that the GUI catches up at its next resync for another reason.
    h.check("after the move: the GUI shows a tab with two panes",
            any(len(p) == 2 for _w, _t, p in tabs(gui)), f"{len(tabs(gui))} tabs")
    same_as_server(h, "after the move", server, gui)

    # A new tab makes every client resync, as any later change in a busy
    # session does.
    h.cli("spawn", "--window-id", str(w1))
    time.sleep(3)
    server, gui = h.server_rows(), h.gui_rows()
    h.save("server-after", server)
    h.save("gui-after", gui)
    same_as_server(h, "after a spawn", server, gui)
    h.check("server kept every pane", len(server) == 6, f"{len(server)} panes")
    return h.failures


def titles_in_order(rows):
    """The tab titles of the first window, one per tab, in list order."""
    window = rows[0]["window_id"]
    by_tab = dict.fromkeys((r["tab_id"], r["tab_title"]) for r in rows if r["window_id"] == window)
    return [title for _tab, title in by_tab]


def order(h):
    first = first_pane(h)
    w = first["window_id"]
    for _ in range(3):
        h.cli("spawn", "--window-id", str(w))
    for title, row in zip("ABCD", h.server_rows()):
        h.cli("set-tab-title", "--tab-id", str(row["tab_id"]), title)
    h.check("server starts with tabs A B C D",
            titles_in_order(h.server_rows()) == list("ABCD"), str(titles_in_order(h.server_rows())))

    # WATCHER_GUI names another wezterm-gui for the GUI that only watches,
    # such as one built before the server could take a tab order.
    watcher = h.attach(gui_bin=os.environ.get("WATCHER_GUI"))
    h.check("first GUI attaches", h.wait_for_gui_tabs(4, watcher))
    mover = h.attach({"PROBE_MOVE_TAB": "0:2", "PROBE_MOVE_AFTER": "3"})
    h.check("second GUI attaches", h.wait_for_gui_tabs(4, mover))
    time.sleep(6)

    moved = list("BCAD")
    server, seen_by_mover, seen_by_watcher = h.server_rows(), h.gui_rows(mover), h.gui_rows(watcher)
    h.save("server-after-move", server)
    h.save("mover-after-move", seen_by_mover)
    h.save("watcher-after-move", seen_by_watcher)
    h.check("the moving GUI shows its move", titles_in_order(seen_by_mover) == moved,
            str(titles_in_order(seen_by_mover)))
    h.check("server took the new order", titles_in_order(server) == moved,
            str(titles_in_order(server)))
    h.check("the other GUI shows the new order", titles_in_order(seen_by_watcher) == moved,
            str(titles_in_order(seen_by_watcher)))

    # A new tab makes every client resync, and a resync must not undo the
    # move, whether or not the server took it.
    e = int(h.cli("spawn", "--window-id", str(w)))
    e_tab = next(r["tab_id"] for r in h.server_rows() if r["pane_id"] == e)
    h.cli("set-tab-title", "--tab-id", str(e_tab), "E")
    h.check("the moving GUI shows the spawned tab", h.wait_for_gui_tabs(5, mover))
    time.sleep(2)
    server, seen_by_mover, seen_by_watcher = h.server_rows(), h.gui_rows(mover), h.gui_rows(watcher)
    h.save("server-after-spawn", server)
    h.save("mover-after-spawn", seen_by_mover)
    h.save("watcher-after-spawn", seen_by_watcher)
    moved = moved + ["E"]
    h.check("after a resync the moving GUI still shows its move",
            titles_in_order(seen_by_mover) == moved, str(titles_in_order(seen_by_mover)))
    h.check("after a resync the server keeps the new order", titles_in_order(server) == moved,
            str(titles_in_order(server)))
    h.check("after a resync the other GUI shows the new order", titles_in_order(seen_by_watcher) == moved,
            str(titles_in_order(seen_by_watcher)))
    # A GUI that predates tab orders shows the old order, but it must stay
    # attached: it still lists the spawned tab, and logs no lost connection.
    h.check("the other GUI is still attached: it lists the spawned tab",
            h.wait_for_gui_tabs(5, watcher, timeout=5))
    lost = [line for line in open(watcher.log) if "process_unilateral" in line or "reconnect" in line.lower()]
    h.check("the other GUI's log has no lost connection", not lost, lost[0].strip()[:160] if lost else "")

    h.detach(mover)
    h.detach(watcher)
    later = h.attach()
    h.check("a GUI attached afterwards lists the tabs", h.wait_for_gui_tabs(5, later))
    seen_later = h.gui_rows(later)
    h.save("later-gui", seen_later)
    h.check("a GUI attached afterwards shows the new order", titles_in_order(seen_later) == moved,
            str(titles_in_order(seen_later)))
    return h.failures


SCENARIOS = {"ghost": ghost, "order": order}

if __name__ == "__main__":
    main()
