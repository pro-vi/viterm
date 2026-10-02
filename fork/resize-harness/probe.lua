-- Config for the resize harness: one private unix domain, no shell, no
-- plugins, so the throwaway server and GUI never touch a real session.
local wezterm = require("wezterm")
local sock = os.getenv("PROBE_SOCK")
assert(sock and sock:match("/resize%-harness[^/]*/"), "PROBE_SOCK must live under a resize-harness directory")
local c = wezterm.config_builder()
c.unix_domains = { { name = "probe", socket_path = sock } }
c.default_prog = { "/bin/sleep", "100000" }
c.automatically_reload_config = false
c.window_close_confirmation = "NeverPrompt"
c.initial_cols = 120
c.initial_rows = 30
c.periodic_stat_logging = tonumber(os.getenv("PROBE_STATS") or "0")

-- Padding the key table makes each format-tab-title call convert a larger
-- config, slowing every tab bar rebuild the way a large real config does.
local pad = tonumber(os.getenv("PROBE_PAD_KEYS") or "0")
if pad > 0 then
  c.keys = {}
  for i = 1, pad do
    table.insert(c.keys, { key = "F" .. ((i % 20) + 1), mods = "CTRL|ALT|SHIFT|SUPER", action = wezterm.action.Nop })
  end
end
wezterm.on("format-tab-title", function(tab) return " " .. (tab.tab_index + 1) .. " " end)

-- Resizes the window through PROBE_SIZES ("WxH,...", physical pixels),
-- PROBE_STEP seconds apart, starting PROBE_START seconds after attach.
wezterm.on("gui-attached", function(domain)
  local sizes = os.getenv("PROBE_SIZES") or ""
  local step = tonumber(os.getenv("PROBE_STEP") or "0.05")
  local t = tonumber(os.getenv("PROBE_START") or "0.3")
  local win = wezterm.gui.gui_windows()[1]
  for w, h in sizes:gmatch("(%d+)x(%d+)") do
    local W, H = tonumber(w), tonumber(h)
    wezterm.time.call_after(t, function() win:set_inner_size(W, H) end)
    t = t + step
  end
  wezterm.log_info("probe: scheduled resizes until t=" .. t)
end)
return c
