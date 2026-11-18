#!/usr/bin/env python3
"""Window name probe: a throwaway mux server on a socket of its own.

    run.py <scenario> <out-dir> <server-dir> [gui-binary]

Scenarios:
  names   the name survives a pane's title escape and a set-window-title, is
          per window, and an empty name removes it (server and CLI from
          server-dir)
  older   server-dir's CLI against the stock server at /Applications/WezTerm.app
          lists windows with no name and refuses set-window-name; the stock CLI
          lists the new server's panes
  gui     a GUI attached to the server names its window through
          MuxWindow:set_name, and the server lists that name

out-dir must not exist or be empty, and must have "window-name-probe" in its
path, as probe.lua asserts. The server's socket and its HOME live in it.
"""
import json, os, pathlib, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
CONFIG = os.path.join(HERE, "probe.lua")
STOCK = "/Applications/WezTerm.app/Contents/MacOS"
NAME = "glass-bead-game"


class Harness:
    def __init__(self, run, srv_dir, cli_dir):
        self.run, self.srv_dir, self.cli_dir = run, srv_dir, cli_dir
        self.sock = os.path.join(run, "sock")
        self.env = {k: v for k, v in os.environ.items()
                    if k not in ("WEZTERM_UNIX_SOCKET", "WEZTERM_PANE", "WEZTERM_CONFIG_FILE")}
        self.env["PROBE_SOCK"] = self.sock
        self.failures, self.guis = [], []
        home = os.path.join(run, "home")
        os.makedirs(home)
        self.server = subprocess.Popen(
            [os.path.join(srv_dir, "wezterm-mux-server"), "--config-file", CONFIG],
            stdout=open(os.path.join(run, "server.log"), "w"), stderr=subprocess.STDOUT,
            env={**self.env, "HOME": home})
        try:
            self.wait(lambda: self.rows(), "the server's first pane")
        except BaseException:
            self.stop()
            raise

    def cli(self, *args, cli_dir=None, check=True):
        r = subprocess.run([os.path.join(cli_dir or self.cli_dir, "wezterm"), "--config-file", CONFIG,
                            "cli", "--no-auto-start", *args],
                           env={**self.env, "WEZTERM_UNIX_SOCKET": self.sock},
                           capture_output=True, text=True, timeout=10)
        if check and r.returncode != 0:
            raise RuntimeError(f"cli {' '.join(args)}: {r.stderr.strip()[:300]}")
        return r

    def rows(self, cli_dir=None):
        try:
            return json.loads(self.cli("list", "--format", "json", cli_dir=cli_dir).stdout)
        except (RuntimeError, json.JSONDecodeError, subprocess.TimeoutExpired):
            return []

    def window(self, window_id, rows=None):
        """(title, name) of a window, from its first listed pane."""
        for r in rows if rows is not None else self.rows():
            if r["window_id"] == window_id:
                return r["window_title"], r.get("window_name")
        return None

    def wait(self, ok, what, timeout=15):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            value = ok()
            if value:
                return value
            time.sleep(0.25)
        raise RuntimeError(f"waited {timeout} s for {what}")

    def check(self, name, ok, detail=""):
        print(f"{'PASS' if ok else 'FAIL'}  {name}" + (f"  ({detail})" if detail else ""))
        if not ok:
            self.failures.append(name)

    def stop(self):
        for gui in self.guis:
            end(gui)
            pathlib.Path(gui.sock).unlink(missing_ok=True)
        end(self.server)


def end(proc):
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()


def names(h):
    w = h.rows()[0]["window_id"]
    h.check("an unnamed window lists an empty name", h.window(w) is not None and h.window(w)[1] == "", str(h.window(w)))
    h.cli("set-window-name", "--window-id", str(w), NAME)
    h.check("set-window-name names the window", h.window(w)[1] == NAME, str(h.window(w)))
    escape = "printf '\\033]2;from-program\\007'; exec sleep 100000"
    h.cli("spawn", "--window-id", str(w), "--", "/bin/sh", "-c", escape)
    title, name = h.wait(lambda: (h.window(w) or (None,))[0] == "from-program" and h.window(w),
                         "the pane's title escape to reach the window title")
    h.check("a pane's title escape changes the title, not the name", name == NAME, f"title {title!r}, name {name!r}")
    h.cli("set-window-title", "--window-id", str(w), "set by cli")
    title, name = h.window(w)
    h.check("set-window-title changes the title, not the name", title == "set by cli" and name == NAME, f"title {title!r}, name {name!r}")
    pane = h.cli("spawn", "--new-window").stdout.strip()
    other = next(r["window_id"] for r in h.rows() if str(r["pane_id"]) == pane)
    h.check("a second window has no name", other != w and h.window(other)[1] == "", str(h.window(other)))
    h.cli("set-window-name", "--pane-id", pane, "pico")
    h.check("--pane-id names the pane's window only", h.window(other)[1] == "pico" and h.window(w)[1] == NAME,
            f"{h.window(w)} {h.window(other)}")
    h.cli("set-window-name", "--window-id", str(w), "")
    h.check("an empty name removes the name", h.window(w)[1] == "", str(h.window(w)))


def older(h):
    stock = [r for r in h.rows(cli_dir=STOCK)]
    h.check("the stock CLI lists the new server's panes", len(stock) > 0 and "window_name" not in stock[0], f"{len(stock)} rows")


def older_server(run, cli_dir):
    h = Harness(run, STOCK, cli_dir)
    try:
        rows = h.rows()
        h.check("the new CLI lists an older server's windows with no name",
                len(rows) > 0 and all(r.get("window_name") == "" for r in rows), f"{len(rows)} rows")
        r = h.cli("set-window-name", "--window-id", str(rows[0]["window_id"]), NAME, check=False)
        h.check("set-window-name against an older server fails and says so",
                r.returncode != 0 and "invalid PDU" in r.stderr, r.stderr.strip()[:200])
    finally:
        h.stop()
    return h.failures


def gui(h, gui_bin):
    log = open(os.path.join(h.run, "gui.log"), "w")
    g = subprocess.Popen([gui_bin, "--config-file", CONFIG, "connect", "probe"],
                         stdout=log, stderr=subprocess.STDOUT, env={**h.env, "PROBE_NAME": NAME})
    g.sock = os.path.expanduser(f"~/.local/share/wezterm/gui-sock-{g.pid}")
    h.guis.append(g)
    try:
        h.wait(lambda: any(r.get("window_name") == NAME for r in h.rows()), "the GUI's name to reach the server", 30)
        h.check("MuxWindow:set_name in an attached GUI names the server's window", True)
    except RuntimeError as err:
        h.check("MuxWindow:set_name in an attached GUI names the server's window", False, str(err))


def main():
    scenario, run, srv_dir = sys.argv[1], os.path.abspath(sys.argv[2]), os.path.abspath(sys.argv[3])
    if "window-name-probe" not in run:
        sys.exit("out-dir must have window-name-probe in its path")
    os.makedirs(run, exist_ok=True)
    if os.listdir(run):
        sys.exit(f"{run} is not empty")
    if scenario == "older":
        failures = older_server(os.path.join(run, "older-server"), srv_dir)
        h = Harness(os.path.join(run, "new-server"), srv_dir, srv_dir)
        try:
            older(h)
        finally:
            h.stop()
        failures += h.failures
    else:
        h = Harness(run, srv_dir, srv_dir)
        try:
            names(h) if scenario == "names" else gui(h, os.path.abspath(sys.argv[4]))
        finally:
            h.stop()
        failures = h.failures
    print(f"{scenario}: {'PASS' if not failures else 'FAIL ' + ', '.join(failures)}")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
