#!/usr/bin/env python3
"""Does a config reload that changes a background gradient keep the image it replaced?

    run.py <path to wezterm-gui> <out-dir> [image-cache-size]

Starts a throwaway GUI from a config of its own (no plugin, no unix domain, the
pane runs sleep) with two gradient layers at 100% of the window, as a
`config.background` with two `Gradient` sources does. It rewrites that config
three times with the same colours, then ten times with new colours, and after
each step reads the GUI's MALLOC_LARGE dirty bytes with `footprint`. The GUI
reloads its config on each rewrite. It kills only the pid it started; the
window appears on screen while it runs. image-cache-size sets
`glyph_cache_image_cache_size` (default 1024, the value this machine's config
uses).

A gradient is computed once per reload at the window's pixel size, with random
noise, so new colours or a new size give an image with a new hash.
`CachedGradient` drops the image it replaced, but the glyph cache's
`image_cache` keeps every image it has drawn until it holds
`glyph_cache_image_cache_size` of them.

Measured 2026-11-25 with engine 20261117-191650-92b446ea, a window of about
2272 x 1580 pixels, two layers:

| image-cache-size | 3 reloads, same colours | 5 with new colours | 10 with new colours |
|---|---|---|---|
| 1024 | +0.0 MB | +140.0 MB | +280.0 MB |
| 4    | +0.0 MB | +56.0 MB  | +56.0 MB  |

28 MB a reload is two images of the window's size, and it stops only when the
cache is full. A fix passes when ten reloads with new colours at 1024 grow by
no more than the two images in use.
"""
import os
import subprocess
import sys
import time

gui, out = sys.argv[1], sys.argv[2]
cache_size = int(sys.argv[3]) if len(sys.argv) > 3 else 1024
if os.path.exists(out):
    sys.exit(f"out-dir already exists: {out}")
os.makedirs(out)
cfg = os.path.join(out, "probe.lua")

TEMPLATE = """local wezterm = require 'wezterm'
local config = wezterm.config_builder()
config.automatically_reload_config = true
config.unix_domains = {{}}
config.default_prog = {{ '/bin/sleep', '900' }}
config.initial_cols = 160
config.initial_rows = 48
config.glyph_cache_image_cache_size = {cache_size}
config.background = {{
  {{ source = {{ Gradient = {{ colors = {{ '#0000{a:02x}', '#101018', '#202030' }}, orientation = 'Vertical' }} }},
    width = '100%', height = '100%', opacity = 0.85 }},
  {{ source = {{ Gradient = {{ colors = {{ '#00{a:02x}00', '#0a0e1a' }}, orientation = 'Vertical' }} }},
    width = '100%', height = '100%', opacity = 0.45 }},
}}
return config
"""


def write(a):
    with open(cfg, "w") as f:
        f.write(TEMPLATE.format(a=a, cache_size=cache_size))


def malloc_large(pid):
    r = subprocess.run(["footprint", "--pid", str(pid), "-f", "bytes"], capture_output=True, text=True, timeout=60)
    for line in r.stdout.splitlines():
        p = line.split()
        if p and p[-1] == "MALLOC_LARGE":
            return int(p[0])
    sys.exit("footprint printed no MALLOC_LARGE row")


write(1)
env = {k: v for k, v in os.environ.items() if k not in ("WEZTERM_UNIX_SOCKET", "WEZTERM_PANE", "WEZTERM_CONFIG_FILE")}
proc = subprocess.Popen(
    [gui, "--config-file", cfg, "start", "--always-new-process", "--class", "gradient-reload-harness"],
    env=env,
    stdout=open(os.path.join(out, "gui.log"), "w"),
    stderr=subprocess.STDOUT,
)
try:
    time.sleep(8)
    base = malloc_large(proc.pid)
    print(f"gui pid {proc.pid}, image cache {cache_size}: MALLOC_LARGE {base / 1048576:.1f} MB at start")
    for _ in range(3):
        write(1)
        time.sleep(3)
    print(f"3 reloads, same colours: {(malloc_large(proc.pid) - base) / 1048576:+.1f} MB")
    for i in range(10):
        write(2 + i)
        time.sleep(3)
        if i in (4, 9):
            print(f"{i + 1} reloads with new colours: {(malloc_large(proc.pid) - base) / 1048576:+.1f} MB")
finally:
    proc.kill()
    proc.wait()
