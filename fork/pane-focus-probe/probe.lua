-- Config for the pane focus probe: one private unix domain, no shell, no
-- plugins, so the throwaway server and GUI never touch a real session.
local wezterm = require("wezterm")
local sock = os.getenv("PROBE_SOCK")
assert(sock and sock:match("/[^/]*pane%-focus%-probe[^/]*/"), "PROBE_SOCK must live under a pane-focus-probe directory")
local c = wezterm.config_builder()
c.unix_domains = { { name = "probe", socket_path = sock, no_serve_automatically = true } }
c.default_prog = { "/bin/sleep", "100000" }
c.automatically_reload_config = false
c.window_close_confirmation = "NeverPrompt"

-- PROBE_ACTIVATE_AFTER seconds after attach, make the other pane of the
-- first split tab active through MuxPane:activate, which calls
-- Tab::set_active_pane as a click on that pane does. Tries once a second
-- until the GUI has learned the split tab.
-- PROBE_INNER_WIDTH resizes every window of this GUI to that many pixels
-- wide two seconds after attach, so two GUIs hold one tab at different
-- sizes: a GUI that attaches takes the server's tab size.
wezterm.on("gui-attached", function(domain)
  local width = tonumber(os.getenv("PROBE_INNER_WIDTH") or "")
  if not width then return end
  wezterm.time.call_after(2, function()
    for _, win in ipairs(wezterm.gui.gui_windows()) do
      local dims = win:get_dimensions()
      win:set_inner_size(width, dims.pixel_height)
    end
    wezterm.log_info("probe: resized to " .. width .. " px")
  end)
end)

wezterm.on("gui-attached", function(domain)
  local after = tonumber(os.getenv("PROBE_ACTIVATE_AFTER") or "")
  if not after then return end
  local tries = 0
  local function try()
    tries = tries + 1
    for _, win in ipairs(wezterm.mux.all_windows()) do
      for _, tab in ipairs(win:tabs()) do
        local panes = tab:panes()
        if #panes > 1 then
          local active = tab:active_pane():pane_id()
          for _, p in ipairs(panes) do
            if p:pane_id() ~= active then
              wezterm.log_info("probe: activating pane " .. p:pane_id())
              p:activate()
              wezterm.log_info("probe: activate returned")
              return
            end
          end
        end
      end
    end
    if tries < 20 then
      wezterm.time.call_after(1, try)
    else
      wezterm.log_info("probe: no split tab found")
    end
  end
  wezterm.time.call_after(after, try)
end)
return c
