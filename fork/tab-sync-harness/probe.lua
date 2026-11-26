-- Config for the tab sync harness: one private unix domain, no shell, no
-- plugins, so the throwaway server and GUI never touch a real session.
local wezterm = require("wezterm")
local sock = os.getenv("PROBE_SOCK")
assert(sock and sock:match("/tab%-sync%-harness[^/]*/"), "PROBE_SOCK must live under a tab-sync-harness directory")
local c = wezterm.config_builder()
-- A GUI that finds no server there fails rather than start one of its own,
-- which would use the real HOME.
c.unix_domains = { { name = "probe", socket_path = sock, no_serve_automatically = true } }
c.default_prog = { "/bin/sleep", "100000" }
c.automatically_reload_config = false
c.window_close_confirmation = "NeverPrompt"
c.initial_cols = 100
c.initial_rows = 24

-- PROBE_MOVE_TAB="<tab index>:<new index>" moves that tab of the first GUI
-- window PROBE_MOVE_AFTER seconds after attach, as the MoveTab key does.
wezterm.on("gui-attached", function(domain)
  local spec = os.getenv("PROBE_MOVE_TAB")
  if not spec then return end
  local from, to = spec:match("^(%d+):(%d+)$")
  local after = tonumber(os.getenv("PROBE_MOVE_AFTER") or "2")
  wezterm.time.call_after(after, function()
    local win = wezterm.gui.gui_windows()[1]
    local tab = win:mux_window():tabs()[tonumber(from) + 1]
    tab:activate()
    win:perform_action(wezterm.action.MoveTab(tonumber(to)), tab:active_pane())
    wezterm.log_info("probe: moved tab " .. from .. " to " .. to)
  end)
end)
return c
