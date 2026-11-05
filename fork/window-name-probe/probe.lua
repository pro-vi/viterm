-- Config for the window name probe: one private unix domain, no shell, no
-- plugins, so the throwaway server and GUI never touch a real session.
local wezterm = require("wezterm")
local sock = os.getenv("PROBE_SOCK")
assert(sock and sock:match("/window%-name%-probe[^/]*/"), "PROBE_SOCK must live under a window-name-probe directory")
local c = wezterm.config_builder()
-- A GUI that finds no server there fails rather than start one of its own,
-- which would use the real HOME.
c.unix_domains = { { name = "probe", socket_path = sock, no_serve_automatically = true } }
c.default_prog = { "/bin/sleep", "100000" }
c.automatically_reload_config = false
c.window_close_confirmation = "NeverPrompt"

-- PROBE_NAME names the first GUI window's mux window two seconds after
-- attach, as a key binding calling set_name would.
wezterm.on("gui-attached", function(domain)
  local name = os.getenv("PROBE_NAME")
  if not name then return end
  wezterm.time.call_after(2, function()
    wezterm.gui.gui_windows()[1]:mux_window():set_name(name)
    wezterm.log_info("probe: named the window " .. name)
  end)
end)
return c
