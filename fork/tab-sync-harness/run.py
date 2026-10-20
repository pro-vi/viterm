#!/usr/bin/env python3
"""Tab sync harness: does a mux-attached GUI show the tabs the server has?

Starts a throwaway mux server from probe.lua, builds two windows of tabs
through its CLI, attaches a GUI, changes the tabs from the server's side, and
compares the tabs the GUI lists with the server's.

    fork/tab-sync-harness/run.py <scenario> <out-dir> <gui-binary> <server-dir>

scenario:
  ghost  a tab is spawned in the second window and its only pane is moved
         into a tab of the first (`split-pane --move-pane-id`), so the server
         removes the emptied tab. The GUI should show the move at once, which
         needs the server to announce it; after its next resync (forced here
         by spawning a tab) it must drop the emptied tab too and show the
         moved pane once.
  order  one window of four titled tabs and two GUIs; the second GUI moves
         the first tab to the third place, as the MoveTab key does. The
         server must take the new order, the first GUI must show it, and a
         GUI attached afterwards must show it too. WATCHER_GUI=<path> runs
         the first GUI from another binary.

out-dir must contain "tab-sync-harness"; the server's socket lives in it.
server-dir holds the wezterm-mux-server and wezterm (CLI) binaries to use.
The GUI window appears on screen while a run is going. Prints PASS or FAIL
per check and exits 1 on any failure.
"""
import json, os, shutil, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
CONFIG = os.path.join(HERE, "probe.lua")


def main():
    if len(sys.argv) != 5:
        sys.exit(__doc__)
    scenario, run, gui_bin, srv_dir = sys.argv[1:]
    if "tab-sync-harness" not in run:
        sys.exit("out-dir must contain tab-sync-harness")
    if scenario not in SCENARIOS:
        sys.exit(f"unknown scenario {scenario}")
    shutil.rmtree(run, ignore_errors=True)
    os.makedirs(run)
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
        self.server = subprocess.Popen(
            [os.path.join(srv_dir, "wezterm-mux-server"), "--config-file", CONFIG],
            stdout=open(os.path.join(run, "server.log"), "w"), stderr=subprocess.STDOUT, env=self.env)
        for _ in range(100):
            if os.path.exists(self.sock):
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("the server made no socket")

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
        log = open(os.path.join(self.run, f"gui-{len(self.guis)}.log"), "w")
        gui = subprocess.Popen(
            [gui_bin or self.gui_bin, "--config-file", CONFIG, "connect", "probe"],
            stdout=log, stderr=subprocess.STDOUT, env={**self.env, **(extra_env or {})})
        gui.sock = os.path.expanduser(f"~/.local/share/wezterm/gui-sock-{gui.pid}")
        self.guis.append(gui)
        return gui

    def detach(self, gui):
        gui.terminate()
        gui.wait(10)
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
            except (RuntimeError, json.JSONDecodeError):
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
        for proc in self.guis + [self.server]:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(5)
                except subprocess.TimeoutExpired:
                    proc.kill()
        for gui in self.guis:
            try:
                os.unlink(gui.sock)
            except FileNotFoundError:
                pass


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
    counts = {}
    for w, _t, _p in tabs(rows):
        counts[w] = counts.get(w, 0) + 1
    return sorted(counts.values())


def first_pane(h):
    """The server's first pane; one is spawned if the server started empty."""
    rows = h.server_rows()
    if not rows:
        h.cli("spawn", "--new-window")
        rows = h.server_rows()
    return rows[0]


def ghost(h):
    first = [first_pane(h)]
    w1, a = first[0]["window_id"], first[0]["pane_id"]
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
            moved["tab_id"] == first[0]["tab_id"] and len(tabs(server)) == 4,
            f"{len(tabs(server))} tabs")
    # The server announces the move only if a split notifies its clients;
    # without that the GUI catches up at its next resync for another reason.
    h.check("GUI shows the move by itself: a tab with two panes",
            any(len(p) == 2 for _w, _t, p in tabs(gui)), f"{len(tabs(gui))} tabs")

    # A new tab makes every client resync, as any later change in a busy
    # session does.
    h.cli("spawn", "--window-id", str(w1))
    time.sleep(3)
    server, gui = h.server_rows(), h.gui_rows()
    h.save("server-after", server)
    h.save("gui-after", gui)
    h.check("GUI lists as many tabs as the server",
            len(tabs(gui)) == len(tabs(server)), f"GUI {len(tabs(gui))}, server {len(tabs(server))}")
    h.check("GUI shows no pane in two tabs", not panes_in_two_tabs(gui), str(panes_in_two_tabs(gui)))
    h.check("GUI windows hold the server's tab counts",
            tabs_per_window(gui) == tabs_per_window(server),
            f"GUI {tabs_per_window(gui)}, server {tabs_per_window(server)}")
    h.check("server kept every pane", len(server) == 6, f"{len(server)} panes")
    return h.failures


def titles_in_order(rows):
    """The tab titles of the first window, in list order."""
    window = rows[0]["window_id"]
    seen = []
    for r in rows:
        if r["window_id"] == window and r["tab_title"] not in seen:
            seen.append(r["tab_title"])
    return seen


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

    h.detach(mover)
    h.detach(watcher)
    later = h.attach()
    h.check("a GUI attached afterwards lists the tabs", h.wait_for_gui_tabs(4, later))
    seen_later = h.gui_rows(later)
    h.save("later-gui", seen_later)
    h.check("a GUI attached afterwards shows the new order", titles_in_order(seen_later) == moved,
            str(titles_in_order(seen_later)))
    return h.failures


SCENARIOS = {"ghost": ghost, "order": order}

if __name__ == "__main__":
    main()
