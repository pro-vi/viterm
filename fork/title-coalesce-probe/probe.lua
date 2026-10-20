-- Throwaway GUI for counting tab bar rebuilds per OSC 0 title write.
-- No plugin, no unix domain unless PROBE_DOMAIN is set; every pane runs sleep.
local wezterm = require 'wezterm'
local config = wezterm.config_builder()
config.automatically_reload_config = false
config.default_prog = { '/bin/sleep', '3600' }
config.use_fancy_tab_bar = true
config.status_update_interval = 3600000
local writes = tonumber(os.getenv('PROBE_WRITES') or '60')
local gap = os.getenv('PROBE_GAP') or '0.5'
local tabs = tonumber(os.getenv('PROBE_TABS') or '21')
local domain = os.getenv('PROBE_DOMAIN')
if domain then
  config.unix_domains = { { name = 'probe', socket_path = domain } }
end
local calls = 0
wezterm.on('format-tab-title', function(tab)
  calls = calls + 1
  return tab.active_pane.title
end)
wezterm.on('user-var-changed', function(window, pane, name, value)
  wezterm.log_info(string.format('PROBE %s=%s format-tab-title calls=%d tabs=%d', name, value, calls, tabs))
end)
local function populate(window)
  for _ = 1, tabs - 2 do window:spawn_tab {} end
  window:spawn_tab { args = { os.getenv('PROBE_WRITER'), tostring(writes), gap, os.getenv('PROBE_DELAY') or '4' } }
end
wezterm.on('gui-startup', function(cmd)
  if domain then return end
  local _, _, window = wezterm.mux.spawn_window {}
  populate(window)
end)
-- With a domain, the throwaway mux server builds the tabs when it starts,
-- and the GUI attaches to them.
wezterm.on('mux-startup', function()
  if not domain then return end
  local _, _, window = wezterm.mux.spawn_window {}
  populate(window)
end)
return config
