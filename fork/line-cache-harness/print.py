#!/usr/bin/env python3
"""Prints numbered lines at a steady rate, in two phases the runner starts.

Usage: print.py <go-file> <done-file> <total-lines> <lines-per-burst> <pause-seconds> <warm-lines>

When <go-file> exists it prints <warm-lines> lines and writes <done-file>.warm.
When <go-file>.main exists it prints <total-lines> more and writes the number
it printed in that phase to <done-file>. It then sleeps so the pane stays open.

Each line is 112 columns with a colour change every 8 cells, so it does not
wrap in the harness's 120-column pane and takes one stable row.
"""
import os
import sys
import time

go, done = sys.argv[1], sys.argv[2]
total, burst, pause, warm = int(sys.argv[3]), int(sys.argv[4]), float(sys.argv[5]), int(sys.argv[6])
serial = 0


def emit(count):
    global serial
    left = count
    while left > 0:
        out = []
        for _ in range(min(burst, left)):
            serial += 1
            cells = "".join(
                "\x1b[%dm%08d" % (31 + (serial + k) % 6, (serial * 13 + k) % 100000000) for k in range(13)
            )
            out.append("%07d %s\x1b[0m\r\n" % (serial, cells))
        left -= len(out)
        sys.stdout.write("".join(out))
        sys.stdout.flush()
        time.sleep(pause)


def wait_for(path):
    while not os.path.exists(path):
        time.sleep(0.1)


wait_for(go)
emit(warm)
with open(done + ".warm", "w") as f:
    f.write(str(warm))
wait_for(go + ".main")
emit(total)
with open(done, "w") as f:
    f.write(str(total))
time.sleep(100000)
