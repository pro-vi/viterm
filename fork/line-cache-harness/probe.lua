-- Config for the line-cache harness: one private unix domain, no shell, no
-- plugins, so the throwaway server and GUI never touch a real session. The
-- server's first pane runs print.py, which prints numbered lines at a steady
-- rate when the runner tells it to.
local wezterm = require("wezterm")
local sock = os.getenv("PROBE_SOCK")
assert(sock and sock:match("/line%-cache%-harness/"), "PROBE_SOCK must live under a line-cache-harness directory")
local c = wezterm.config_builder()
c.unix_domains = { { name = "probe", socket_path = sock } }
c.default_prog = {
  "/usr/bin/python3",
  os.getenv("PROBE_PRINT"),
  os.getenv("PROBE_GO"),
  os.getenv("PROBE_DONE"),
  os.getenv("PROBE_TOTAL"),
  os.getenv("PROBE_BURST"),
  os.getenv("PROBE_PAUSE"),
  os.getenv("PROBE_WARM"),
}
c.automatically_reload_config = false
c.window_close_confirmation = "NeverPrompt"
c.initial_cols = 120
c.initial_rows = 30
-- The client sizes each pane's line cache from scrollback_lines (128 at the
-- least), so a small value makes a cache that outgrows it easy to see.
c.scrollback_lines = tonumber(os.getenv("PROBE_SCROLLBACK") or "1000")
c.ratelimit_mux_line_prefetches_per_second = 750
c.periodic_stat_logging = 10

-- One window resize, PROBE_RESIZE_AT seconds after attach.
wezterm.on("gui-attached", function()
  local at = tonumber(os.getenv("PROBE_RESIZE_AT") or "")
  if not at then
    return
  end
  local win = wezterm.gui.gui_windows()[1]
  wezterm.time.call_after(at, function()
    win:set_inner_size(1300, 850)
    wezterm.log_info("probe: resized the window")
  end)
end)
return c
