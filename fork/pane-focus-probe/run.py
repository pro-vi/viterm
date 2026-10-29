#!/usr/bin/env python3
"""Pane focus probe: does a GUI attached to a mux server keep answering after
it makes another pane of a split tab active?

    run.py <out-dir> <bin-dir>

bin-dir holds wezterm-gui, wezterm-mux-server and wezterm from one build. The
server gets a tab split in two; the GUI attaches with `connect` and activates
the inactive pane through MuxPane:activate; then `wezterm cli list` asks the
GUI's own socket for its panes. Exit 0 when it answers within 10 s, 1 when it
does not, and then `sample` writes the GUI's stacks to out-dir/sample.txt.

out-dir must not exist or be empty, and must have "pane-focus-probe" in its
path, as probe.lua asserts. The server's socket and its HOME live in it.
"""
import os, pathlib, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
CONFIG = os.path.join(HERE, "probe.lua")


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    run, bins = os.path.abspath(sys.argv[1]), sys.argv[2]
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
        log_path = os.path.join(run, "gui.log")
        gui = subprocess.Popen(
            [os.path.join(bins, "wezterm-gui"), "--config-file", CONFIG, "connect", "probe"],
            stdout=open(log_path, "w"), stderr=subprocess.STDOUT,
            env={**env, "PROBE_ACTIVATE_AFTER": "4", "WEZTERM_LOG": "info"})
        procs.append(gui)
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
            ok = cli("list", to=os.path.expanduser(f"~/.local/share/wezterm/gui-sock-{gui.pid}")).returncode == 0
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


if __name__ == "__main__":
    main()
