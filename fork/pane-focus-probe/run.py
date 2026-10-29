#!/usr/bin/env python3
"""Pane focus probe: a throwaway mux server with a tab split in two, and GUIs
attached to it with `connect`.

    run.py <scenario> <out-dir> <bin-dir>

Scenarios:
  activate  one GUI activates the inactive pane through MuxPane:activate,
            which calls Tab::set_active_pane as a click does, then must
            answer `wezterm cli list` on its own socket within 10 s; when it
            does not, `sample` writes its stacks to out-dir/sample.txt
  follow    two GUIs of different widths show the tab; the first one
            activates each pane in turn four times, and after each the
            server must hold both panes at the first GUI's sizes, since
            that GUI is the one in use

bin-dir holds wezterm-gui, wezterm-mux-server and wezterm from one build.
out-dir must not exist or be empty, and must have "pane-focus-probe" in its
path, as probe.lua asserts. The server's socket and its HOME live in it.
"""
import json, os, pathlib, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
CONFIG = os.path.join(HERE, "probe.lua")


def main():
    if len(sys.argv) != 4 or sys.argv[1] not in ("activate", "follow"):
        sys.exit(__doc__)
    scenario, run, bins = sys.argv[1], os.path.abspath(sys.argv[2]), sys.argv[3]
    if os.path.isdir(run) and os.listdir(run):
        sys.exit(f"out-dir {run} is not empty; give a new one")
    os.makedirs(run, exist_ok=True)
    sock = os.path.join(run, "sock")
    env = {k: v for k, v in os.environ.items()
           if k not in ("WEZTERM_UNIX_SOCKET", "WEZTERM_PANE", "WEZTERM_CONFIG_FILE")}
    env["PROBE_SOCK"] = sock
    home = os.path.join(run, "home")
    os.makedirs(home)
    procs = []

    def cli(*args, to=sock):
        return subprocess.run(
            [os.path.join(bins, "wezterm"), "--config-file", CONFIG, "cli", "--no-auto-start", *args],
            env={**env, "WEZTERM_UNIX_SOCKET": to}, capture_output=True, text=True, timeout=10)

    try:
        procs.append(subprocess.Popen(
            [os.path.join(bins, "wezterm-mux-server"), "--config-file", CONFIG],
            stdout=open(os.path.join(run, "server.log"), "w"), stderr=subprocess.STDOUT,
            env={**env, "HOME": home}))
        for _ in range(100):
            if os.path.exists(sock):
                break
            time.sleep(0.1)
        else:
            sys.exit("the server made no socket in 10 s")
        first = cli("spawn", "--new-window", "--", "/bin/sleep", "100000")
        if first.returncode != 0:
            sys.exit(f"spawn: {first.stderr.strip()[:300]}")
        split = cli("split-pane", "--pane-id", first.stdout.strip(), "--right", "--", "/bin/sleep", "100000")
        if split.returncode != 0:
            sys.exit(f"split-pane: {split.stderr.strip()[:300]}")
        def attach(name, extra):
            log = os.path.join(run, f"{name}.log")
            gui = subprocess.Popen(
                [os.path.join(bins, "wezterm-gui"), "--config-file", CONFIG, "connect", "probe"],
                stdout=open(log, "w"), stderr=subprocess.STDOUT,
                env={**env, "WEZTERM_LOG": "info", **extra})
            procs.append(gui)
            gui.sock = os.path.expanduser(f"~/.local/share/wezterm/gui-sock-{gui.pid}")
            return gui, log

        if scenario == "follow":
            sys.exit(follow(cli, attach, first.stdout.strip(), split.stdout.strip()))
        gui, log_path = attach("gui", {"PROBE_ACTIVATE_AFTER": "4"})
        for _ in range(60):
            log = open(log_path).read()
            if "probe: activating" in log or "probe: no split" in log:
                break
            time.sleep(0.5)
        else:
            sys.exit("the GUI logged no probe line in 30 s")
        if "probe: activating" not in log:
            sys.exit("the GUI found no split tab")
        time.sleep(2)
        try:
            ok = cli("list", to=gui.sock).returncode == 0
        except subprocess.TimeoutExpired:
            ok = False
        returned = "probe: activate returned" in open(log_path).read()
        if not ok:
            subprocess.run(["sample", str(gui.pid), "2", "-file", os.path.join(run, "sample.txt")],
                           capture_output=True)
        print(f"activate returned: {returned}; GUI answered after it: {ok}")
        print("PASS" if ok else "FAIL: the GUI stopped answering after the pane became active")
        sys.exit(0 if ok else 1)
    finally:
        for p in reversed(procs):
            if p.poll() is None:
                p.terminate()
                try:
                    p.wait(5)
                except subprocess.TimeoutExpired:
                    p.kill()
                    p.wait()
        for p in procs[1:]:
            pathlib.Path(os.path.expanduser(f"~/.local/share/wezterm/gui-sock-{p.pid}")).unlink(missing_ok=True)



def follow(cli, attach, left, right):
    """Exit status of the follow scenario; left and right are the server's
    pane ids of the split tab."""
    def split_sizes(sock=None):
        """Sizes of the panes of the tab that has two, sorted, and their
        ids, from the list on sock (the server's when None)."""
        args = ("list", "--format", "json")
        r = cli(*args) if sock is None else cli(*args, to=sock)
        if r.returncode != 0:
            return None, None
        tabs = {}
        for row in json.loads(r.stdout):
            tabs.setdefault(row["tab_id"], []).append(row)
        rows = next((t for t in tabs.values() if len(t) == 2), None)
        if rows is None:
            return None, None
        return sorted((row["size"]["cols"], row["size"]["rows"]) for row in rows), [row["pane_id"] for row in rows]

    a, _ = attach("gui-a", {})
    time.sleep(6)
    b, _ = attach("gui-b", {"PROBE_INNER_WIDTH": "1600"})
    time.sleep(6)
    a_sizes, a_ids = split_sizes(a.sock)
    b_sizes, _ = split_sizes(b.sock)
    print(f"first GUI {a_sizes}, second GUI {b_sizes}, server {split_sizes()[0]}")
    if a_sizes is None or b_sizes is None or a_sizes == b_sizes:
        print("SETUP: the two GUIs do not hold the split tab at different sizes")
        return 2
    held_wrong = 0
    for turn in range(4):
        target = a_ids[turn % 2]
        r = cli("activate-pane", "--pane-id", str(target), to=a.sock)
        if r.returncode != 0:
            print(f"FAIL: activate-pane {target}: {r.stderr.strip()[:200]}")
            return 1
        time.sleep(3)
        mine, server = split_sizes(a.sock)[0], split_sizes()[0]
        ok = mine is not None and mine == server
        held_wrong += not ok
        print(f"turn {turn + 1}: first GUI {mine}, server {server}: {'same' if ok else 'DIFFERENT'}")
    print("PASS" if not held_wrong else f"FAIL: the server held other sizes after {held_wrong} of 4 turns")
    return 1 if held_wrong else 0

if __name__ == "__main__":
    main()
