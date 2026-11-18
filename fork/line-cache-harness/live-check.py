#!/usr/bin/env python3
"""Check a live ViTerm window's memory against what the line-cache fix promises, touching nothing.

    live-check.py [--scrollback-lines N] [--samples DIR] [--fixed-from STAMP]

For each wezterm-gui attached to a mux domain it prints the engine the process
runs, how long it has been up, its memory footprint, and the ceiling the fix
promises: every pane's cache full of lines as heavy as the harness prints.
When the mux recorder's samples are present it adds the footprint's change
over the days they cover. The README beside this file says what the fix
should do and what would show that it does not.

Verdicts, one per window:
  NOT FIXED  the window runs an engine older than the first one with the fix
  TOO EARLY  it has the fix but has been up for less than 14 days
  FAIL       its footprint is over the ceiling
  PASS       its footprint is under the ceiling after 14 days or more
Exit status is 0 when every window passes, 1 on a FAIL, 2 otherwise.

--scrollback-lines is the window's `scrollback_lines` (default 50000), which
sizes each pane's cache. --samples is the recorder's folder of
samples-YYYY-MM-DD.ndjson files, one JSON object a line with `time` and
`top_memory.by[]` rows of `pgid`, `name`, `footprint_mb`; the recorder lists
only the five largest process groups, so a window missing from a sample was
smaller than the smallest group listed there. --fixed-from replaces the
version stamp (YYYYMMDD-HHMMSS) of the first engine that has the fix, for a
window that runs a build made elsewhere.
"""
import argparse
import ctypes
import datetime
import glob
import json
import os
import re
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from footprint import footprint_bytes, libproc  # noqa: E402

HOME = os.path.expanduser("~")
ENGINE = f"{HOME}/.local/opt/viterm-engine.app/Contents/MacOS"
SOCKET = f"{HOME}/.local/share/wezterm/human-main.sock"
SAMPLES = f"{HOME}/.local/state/wezterm-mux-recorder"
# The first installed engine that carries wezterm/wezterm#7704.
FIXED_FROM = "20261117-191650"
MIN_DAYS = 14
# What the harness measured for one cached line (112 columns, a colour change
# every 8 cells) and for a window that has cached nothing yet.
BYTES_PER_LINE = 3600
BASE_BYTES = 500 * 1048576
GB = 1073741824


def windows():
    """(pid, start time, executable path) of each wezterm-gui attached to a mux domain."""
    rows = []
    listing = subprocess.run(["ps", "-axo", "pid=,lstart=,command="], capture_output=True, text=True).stdout
    for line in listing.splitlines():
        m = re.match(r"\s*(\d+)\s+(\w+ \w+\s+\d+ [\d:]+ \d+)\s+(.*)", line)
        if not m or "wezterm-gui" not in m.group(3).split(" ")[0] or " connect " not in m.group(3) + " ":
            continue
        pid = int(m.group(1))
        started = time.mktime(time.strptime(re.sub(r"\s+", " ", m.group(2)), "%a %b %d %H:%M:%S %Y"))
        buf = ctypes.create_string_buffer(4096)
        libproc.proc_pidpath(pid, buf, len(buf))
        rows.append((pid, started, buf.value.decode() or m.group(3).split(" ")[0]))
    return rows


def engine_version(executable):
    """The version of the engine a process runs: its own bundle's `wezterm --version`."""
    cli = os.path.join(os.path.dirname(executable), "wezterm")
    try:
        out = subprocess.run([cli, "--version"], capture_output=True, text=True, timeout=10).stdout.split()
    except (OSError, subprocess.TimeoutExpired):
        return None
    return out[1] if len(out) > 1 else None


def pane_count():
    env = dict(os.environ, WEZTERM_UNIX_SOCKET=SOCKET)
    try:
        out = subprocess.run([f"{ENGINE}/wezterm", "cli", "--no-auto-start", "list", "--format", "json"],
                             capture_output=True, text=True, env=env, timeout=20).stdout
        return len(json.loads(out))
    except (OSError, ValueError, subprocess.TimeoutExpired):
        return None


def recorded(samples_dir, pid, since):
    """([(time, footprint GB)] where the window was listed, the largest floor it stayed under otherwise)."""
    listed, unlisted_floor = [], None
    for path in sorted(glob.glob(os.path.join(samples_dir, "samples-*.ndjson"))):
        with open(path) as fh:
            for line in fh:
                try:
                    sample = json.loads(line)
                    when = datetime.datetime.fromisoformat(sample["time"]).timestamp()
                except (ValueError, KeyError):
                    continue
                groups = (sample.get("top_memory") or {}).get("by") or []
                if when < since or not groups:
                    continue
                mine = [g["footprint_mb"] for g in groups if g.get("pgid") == pid and g.get("name") == "wezterm-gui"]
                if mine:
                    listed.append((when, mine[0] / 1024))
                else:
                    floor = min(g["footprint_mb"] for g in groups) / 1024
                    unlisted_floor = floor if unlisted_floor is None else max(unlisted_floor, floor)
    return listed, unlisted_floor


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--scrollback-lines", type=int, default=50000)
    parser.add_argument("--samples", default=SAMPLES)
    parser.add_argument("--fixed-from", default=FIXED_FROM)
    args = parser.parse_args()

    found = windows()
    if not found:
        print("no wezterm-gui attached to a mux domain is running")
        return 2
    panes = pane_count()
    ceiling = None if panes is None else BASE_BYTES + panes * args.scrollback_lines * BYTES_PER_LINE
    print(f"panes on the server: {panes if panes is not None else 'no answer'}; "
          f"cache limit {args.scrollback_lines} lines a pane")
    if ceiling is not None:
        print(f"ceiling: {ceiling / GB:.1f} GB (every pane's cache full, {BYTES_PER_LINE} bytes a line)")

    verdicts = []
    now = time.time()
    for pid, started, executable in found:
        version = engine_version(executable)
        days = (now - started) / 86400
        size = footprint_bytes(pid)
        print(f"\nwindow pid {pid}: engine {version or 'unknown'}, up {days:.1f} days, "
              f"footprint {size / GB:.1f} GB" if size is not None else f"\nwindow pid {pid}: footprint not readable")
        if size is not None and days > 0:
            print(f"  average since launch: {size / GB / days:.2f} GB a day")
        listed, unlisted_floor = recorded(args.samples, pid, started) if os.path.isdir(args.samples) else ([], None)
        if listed:
            (t0, g0), (t1, g1) = listed[0], listed[-1]
            span = (t1 - t0) / 86400
            rate = f", {(g1 - g0) / span:+.2f} GB a day" if span >= 1 else ""
            print(f"  recorder: {g0:.1f} GB -> {g1:.1f} GB over {span:.1f} days{rate}; "
                  f"largest {max(g for _, g in listed):.1f} GB")
        if unlisted_floor is not None:
            print(f"  recorder: in some samples it was not among the five largest groups, "
                  f"so it was under {unlisted_floor:.1f} GB then")
        if version is None or version[:15] < args.fixed_from:
            verdict = "NOT FIXED"
            print(f"  {verdict}: this engine is older than {args.fixed_from}, the first with the fix; relaunch the window")
        elif size is None or ceiling is None:
            verdict = "TOO EARLY"
            print(f"  {verdict}: a reading is missing")
        elif size > ceiling:
            verdict = "FAIL"
            print(f"  {verdict}: {size / GB:.1f} GB is over the ceiling of {ceiling / GB:.1f} GB")
        elif days < MIN_DAYS:
            verdict = "TOO EARLY"
            ready = datetime.date.fromtimestamp(started + MIN_DAYS * 86400)
            print(f"  {verdict}: under the ceiling, but up for less than {MIN_DAYS} days; run again from {ready}")
        else:
            verdict = "PASS"
            print(f"  {verdict}: {size / GB:.1f} GB is under the ceiling of {ceiling / GB:.1f} GB after {days:.0f} days")
        verdicts.append(verdict)
    if "FAIL" in verdicts:
        return 1
    return 0 if all(v == "PASS" for v in verdicts) else 2


if __name__ == "__main__":
    sys.exit(main())
