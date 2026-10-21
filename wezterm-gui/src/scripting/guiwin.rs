//! GuiWin represents a Gui TermWindow (as opposed to a Mux window) in lua code
use super::luaerr;
use crate::termwindow::TermWindowNotif;
use crate::TermWindow;
use config::keyassignment::{ClipboardCopyDestination, KeyAssignment};
use luahelper::*;
use mlua::{UserData, UserDataMethods, UserDataRef};
use mux::pane::PaneId;
use mux::window::WindowId as MuxWindowId;
use mux::Mux;
use mux_lua::MuxPane;
use termwiz_funcs::lines_to_escapes;
use wezterm_dynamic::{FromDynamic, ToDynamic};
use wezterm_toast_notification::ToastNotification;
use window::{Connection, ConnectionOps, DeadKeyStatus, WindowOps, WindowState};

#[derive(Clone)]
pub struct GuiWin {
    pub mux_window_id: MuxWindowId,
    pub window: ::window::Window,
}

impl GuiWin {
    pub fn new(term_window: &TermWindow) -> Self {
        let window = term_window.window.clone().unwrap();
        let mux_window_id = term_window.mux_window_id;
        Self {
            window,
            mux_window_id,
        }
    }
}

impl UserData for GuiWin {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, _: ()| {
            Ok(format!(
                "GuiWin(mux_window_id:{}, pid:{})",
                this.mux_window_id,
                unsafe { libc::getpid() }
            ))
        });

        methods.add_method("window_id", |_, this, _: ()| Ok(this.mux_window_id));
        methods.add_method("mux_window", |_, this, _: ()| {
            Ok(mux_lua::MuxWindow(this.mux_window_id))
        });
        methods.add_method("active_tab", |_, this, _: ()| {
            let mux = Mux::try_get().ok_or_else(|| mlua::Error::external("cannot get Mux!?"))?;
            let window = mux.get_window(this.mux_window_id).ok_or_else(|| {
                mlua::Error::external(format!("invalid window {}", this.mux_window_id))
            })?;
            Ok(window
                .get_active_tab()
                .map(|tab| mux_lua::MuxTab(tab.tab_id())))
        });

        methods.add_method(
            "set_inner_size",
            |_, this, (width, height): (usize, usize)| {
                this.window
                    .notify(TermWindowNotif::SetInnerSize { width, height });
                Ok(())
            },
        );
        methods.add_method("set_position", |_, this, (x, y): (isize, isize)| {
            this.window.set_window_position(euclid::point2(x, y));
            Ok(())
        });
        methods.add_method("maximize", |_, this, _: ()| {
            this.window.maximize();
            Ok(())
        });
        methods.add_method("restore", |_, this, _: ()| {
            this.window.restore();
            Ok(())
        });
        methods.add_method("toggle_fullscreen", |_, this, _: ()| {
            this.window.toggle_fullscreen();
            Ok(())
        });
        methods.add_method("focus", |_, this, _: ()| {
            this.window.focus();
            Ok(())
        });
        methods.add_method(
            "toast_notification",
            |_, _, (title, message, url, timeout): (String, String, Option<String>, Option<u64>)| {
                wezterm_toast_notification::show(ToastNotification {
                    title,
                    message,
                    url,
                    timeout: timeout.map(std::time::Duration::from_millis)
                });
                Ok(())
            },
        );
        methods.add_method("get_appearance", |_, _, _: ()| {
            Ok(Connection::get().unwrap().get_appearance().to_string())
        });
        methods.add_method(
            "set_right_status",
            |_, this, (status, row): (String, Option<usize>)| {
                // `row` counts from 1 to match tab_bar_rows; it is the row the
                // status belongs to and defaults to the first.
                let row = row.unwrap_or(1).saturating_sub(1);
                this.window
                    .notify(TermWindowNotif::SetRightStatus { status, row });
                Ok(())
            },
        );
        methods.add_async_method("toggle_tab_group_popup", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window.notify(TermWindowNotif::ToggleTabGroupPopup(tx));
            rx.recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)
        });
        methods.add_async_method("show_tab_group_hints", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window.notify(TermWindowNotif::ShowTabGroupHints(tx));
            rx.recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)
        });
        methods.add_async_method(
            "open_tab_group_popup",
            |_, this, tab_index: mlua::Value| async move {
                let tab_index = parse_tab_index("open_tab_group_popup", tab_index)?;
                let (tx, rx) = smol::channel::bounded(1);
                this.window
                    .notify(TermWindowNotif::OpenTabGroupPopup(tab_index, tx));
                rx.recv()
                    .await
                    .map_err(|e| anyhow::anyhow!("{:#}", e))
                    .map_err(luaerr)
            },
        );
        methods.add_method("set_tab_groups", |_, this, groups: mlua::Value| {
            let groups = parse_tab_groups(groups)?;
            this.window.notify(TermWindowNotif::SetTabGroups(groups));
            Ok(())
        });
        methods.add_method(
            "set_left_status",
            |_, this, (status, row): (String, Option<usize>)| {
                let row = row.unwrap_or(1).saturating_sub(1);
                this.window
                    .notify(TermWindowNotif::SetLeftStatus { status, row });
                Ok(())
            },
        );
        methods.add_async_method("get_dimensions", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window.notify(TermWindowNotif::GetDimensions(tx));
            let (dims, window_state) = rx
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)?;

            #[derive(FromDynamic, ToDynamic)]
            struct Dims {
                pixel_width: usize,
                pixel_height: usize,
                dpi: usize,
                is_full_screen: bool,
            }
            impl_lua_conversion_dynamic!(Dims);

            let dims = Dims {
                pixel_width: dims.pixel_width,
                pixel_height: dims.pixel_height,
                dpi: dims.dpi,
                is_full_screen: window_state.contains(WindowState::FULL_SCREEN),
                // FIXME: expose other states here
            };
            Ok(dims)
        });
        methods.add_async_method(
            "get_selection_text_for_pane",
            |_, this, pane: UserDataRef<MuxPane>| async move {
                let (tx, rx) = smol::channel::bounded(1);
                this.window.notify(TermWindowNotif::GetSelectionForPane {
                    pane_id: pane.0,
                    tx,
                });
                let text = rx
                    .recv()
                    .await
                    .map_err(|e| anyhow::anyhow!("{:#}", e))
                    .map_err(luaerr)?;

                Ok(text)
            },
        );
        methods.add_async_method("current_event", |lua, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window
                .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                    tx.try_send(term_window.current_event.to_dynamic()).ok();
                })));
            let result = rx.recv().await.map_err(mlua::Error::external)?;
            luahelper::dynamic_to_lua_value(lua, result)
        });
        methods.add_async_method(
            "perform_action",
            |_, this, (assignment, pane): (KeyAssignment, UserDataRef<MuxPane>)| async move {
                let (tx, rx) = smol::channel::bounded(1);
                this.window.notify(TermWindowNotif::PerformAssignment {
                    pane_id: pane.0,
                    assignment,
                    tx: Some(tx),
                });
                let result = rx.recv().await.map_err(mlua::Error::external)?;
                result.map_err(mlua::Error::external)
            },
        );
        methods.add_async_method("effective_config", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window.notify(TermWindowNotif::GetEffectiveConfig(tx));
            let config = rx
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)?;

            Ok((*config).clone())
        });
        methods.add_async_method("get_config_overrides", |lua, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window.notify(TermWindowNotif::GetConfigOverrides(tx));
            let overrides = rx
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)?;

            dynamic_to_lua_value(lua, overrides)
        });
        methods.add_method("set_config_overrides", |_, this, value: mlua::Value| {
            let value = lua_value_to_dynamic(value)?;
            this.window
                .notify(TermWindowNotif::SetConfigOverrides(value));
            Ok(())
        });
        methods.add_async_method("is_focused", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window
                .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                    tx.try_send(term_window.focused.is_some()).ok();
                })));
            let result = rx
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)?;

            Ok(result)
        });
        methods.add_async_method("leader_is_active", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window
                .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                    tx.try_send(term_window.leader_is_active()).ok();
                })));
            let result = rx
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)?;

            Ok(result)
        });
        methods.add_async_method("composition_status", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window
                .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                    tx.try_send(match term_window.composition_status() {
                        DeadKeyStatus::None => None,
                        DeadKeyStatus::Composing(s) => Some(s.clone()),
                    })
                    .ok();
                })));
            let result = rx
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)?;

            Ok(result)
        });
        methods.add_async_method("tab_bar_fit", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window
                .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                    tx.try_send(term_window.tab_bar_fit.clone()).ok();
                })));
            let fit = rx
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)?;

            #[derive(FromDynamic, ToDynamic)]
            struct Fit {
                rows: usize,
                rows_min: Option<usize>,
                rows_full: Option<usize>,
                title_width: usize,
                tabs_per_row: Vec<usize>,
            }
            impl_lua_conversion_dynamic!(Fit);

            Ok(fit.map(|fit| Fit {
                rows: fit.rows,
                rows_min: fit.rows_min,
                rows_full: fit.rows_full,
                title_width: fit.title_width,
                tabs_per_row: fit.tabs_per_row,
            }))
        });
        methods.add_async_method("active_key_table", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window
                .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                    tx.try_send(term_window.current_key_table_name()).ok();
                })));
            let result = rx
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)?;

            Ok(result)
        });
        methods.add_async_method("keyboard_modifiers", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window
                .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                    tx.try_send(term_window.current_modifier_and_led_state())
                        .ok();
                })));
            let (mods, leds) = rx
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)?;

            Ok((mods.to_string(), leds.to_string()))
        });
        methods.add_async_method("active_pane", |_, this, _: ()| async move {
            let (tx, rx) = smol::channel::bounded(1);
            this.window
                .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                    tx.try_send(
                        term_window
                            .get_active_pane_or_overlay()
                            .map(|pane| MuxPane(pane.pane_id())),
                    )
                    .ok();
                })));
            let result = rx
                .recv()
                .await
                .map_err(|e| anyhow::anyhow!("{:#}", e))
                .map_err(luaerr)?;

            Ok(result)
        });
        methods.add_method("active_workspace", |_, _, _: ()| {
            let mux = Mux::try_get()
                .ok_or_else(|| anyhow::anyhow!("no mux?"))
                .map_err(luaerr)?;
            Ok(mux.active_workspace().to_string())
        });
        methods.add_method(
            "copy_to_clipboard",
            |_, this, (text, clipboard): (String, Option<ClipboardCopyDestination>)| {
                let clipboard = clipboard.unwrap_or_default();
                this.window
                    .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                        term_window.copy_to_clipboard(clipboard, text);
                    })));
                Ok(())
            },
        );
        methods.add_async_method(
            "get_selection_escapes_for_pane",
            |_, this, pane: UserDataRef<MuxPane>| async move {
                let (tx, rx) = smol::channel::bounded(1);
                let pane_id = pane.0;
                this.window
                    .notify(TermWindowNotif::Apply(Box::new(move |term_window| {
                        fn do_it(
                            pane_id: PaneId,
                            term_window: &mut TermWindow,
                        ) -> anyhow::Result<String> {
                            let mux = Mux::try_get().ok_or_else(|| anyhow::anyhow!("no mux"))?;
                            let pane = mux
                                .get_pane(pane_id)
                                .ok_or_else(|| anyhow::anyhow!("invalid pane {pane_id}"))?;
                            let lines = term_window.selection_lines(&pane);
                            lines_to_escapes(lines)
                        }
                        tx.try_send(do_it(pane_id, term_window).map_err(|err| format!("{err:#}")))
                            .ok();
                    })));
                let result = rx.recv().await.map_err(mlua::Error::external)?;

                Ok(result)
            },
        );
    }
}

/// A 0-based tab index, as `ActivateTab` takes it. Anything that is not a
/// non-negative whole number is an error that names the method.
fn parse_tab_index(method: &str, value: mlua::Value) -> mlua::Result<usize> {
    match value {
        mlua::Value::Integer(i) if i >= 0 => Ok(i as usize),
        mlua::Value::Number(n) if n >= 0. && n.fract() == 0. => Ok(n as usize),
        _ => Err(mlua::Error::external(format!(
            "{method}: tab_index must be a whole number, 0 or more"
        ))),
    }
}

/// The groups `window:set_tab_groups` was given: a list of
/// `{ tabs = {tab_id, ...}, label = "..." }`. Anything else is an error that
/// names the entry, so a mistake shows in the log instead of as a tab bar
/// that silently stays as it was.
fn parse_tab_groups(groups: mlua::Value) -> mlua::Result<Vec<crate::tabbar::TabGroupSpec>> {
    let fail = |msg: String| mlua::Error::external(format!("set_tab_groups: {msg}"));
    let groups = match groups {
        mlua::Value::Table(t) => t,
        _ => return Err(fail("expected a list of groups".to_string())),
    };
    let mut specs = vec![];
    for (i, entry) in groups.sequence_values::<mlua::Value>().enumerate() {
        let n = i + 1;
        let entry = match entry? {
            mlua::Value::Table(t) => t,
            _ => return Err(fail(format!("entry {n} is not a table"))),
        };
        let label = match entry.get::<_, mlua::Value>("label")? {
            mlua::Value::String(s) => s.to_str()?.to_string(),
            _ => return Err(fail(format!("entry {n}: label must be a string"))),
        };
        let ids = match entry.get::<_, mlua::Value>("tabs")? {
            mlua::Value::Table(t) => t,
            _ => return Err(fail(format!("entry {n}: tabs must be a list of tab ids"))),
        };
        let mut tabs = vec![];
        for id in ids.sequence_values::<mlua::Value>() {
            match id? {
                mlua::Value::Integer(id) if id >= 0 => tabs.push(id as mux::tab::TabId),
                mlua::Value::Number(id) if id >= 0. && id.fract() == 0. => {
                    tabs.push(id as mux::tab::TabId)
                }
                _ => return Err(fail(format!("entry {n}: tabs must be a list of tab ids"))),
            }
        }
        let marks = match entry.get::<_, mlua::Value>("marks")? {
            mlua::Value::Nil => vec![],
            mlua::Value::Table(list) => {
                let bad = || {
                    fail(format!(
                        "entry {n}: marks must be a list of {{ tab = tab_id, glyph = text }} for tabs of the group"
                    ))
                };
                let mut marks = vec![];
                for mark in list.sequence_values::<mlua::Value>() {
                    let mark = match mark? {
                        mlua::Value::Table(m) => m,
                        _ => return Err(bad()),
                    };
                    let tab = match mark.get::<_, mlua::Value>("tab")? {
                        mlua::Value::Integer(id) if id >= 0 => id as mux::tab::TabId,
                        mlua::Value::Number(id) if id >= 0. && id.fract() == 0. => {
                            id as mux::tab::TabId
                        }
                        _ => return Err(bad()),
                    };
                    let glyph = match mark.get::<_, mlua::Value>("glyph")? {
                        mlua::Value::String(g) if !g.as_bytes().is_empty() => g.to_str()?.to_string(),
                        _ => return Err(bad()),
                    };
                    if !tabs.contains(&tab) {
                        return Err(bad());
                    }
                    marks.push(crate::tabbar::TabGroupMark { tab, glyph });
                }
                marks
            }
            _ => {
                return Err(fail(format!(
                    "entry {n}: marks must be a list of {{ tab = tab_id, glyph = text }} for tabs of the group"
                )))
            }
        };
        let busy = match entry.get::<_, mlua::Value>("busy")? {
            mlua::Value::Nil => None,
            mlua::Value::String(b) => Some(b.to_str()?.to_string()),
            _ => return Err(fail(format!("entry {n}: busy must be a string"))),
        };
        specs.push(crate::tabbar::TabGroupSpec {
            tabs,
            label,
            marks,
            busy,
        });
    }
    Ok(specs)
}

#[cfg(test)]
mod test {
    use super::*;

    fn parse(src: &str) -> mlua::Result<Vec<crate::tabbar::TabGroupSpec>> {
        let lua = mlua::Lua::new();
        let value: mlua::Value = lua.load(src).eval().unwrap();
        parse_tab_groups(value)
    }

    fn index(src: &str) -> mlua::Result<usize> {
        let lua = mlua::Lua::new();
        let value: mlua::Value = lua.load(src).eval().unwrap();
        parse_tab_index("open_tab_group_popup", value)
    }

    #[test]
    fn a_tab_index_is_a_whole_number_from_zero() {
        assert_eq!(index("0").unwrap(), 0);
        assert_eq!(index("7").unwrap(), 7);
        assert_eq!(index("7.0").unwrap(), 7);
    }

    #[test]
    fn a_tab_index_that_is_negative_fractional_or_not_a_number_is_refused() {
        for src in ["-1", "1.5", "'3'", "nil", "{}"] {
            let err = index(src).unwrap_err().to_string();
            assert!(err.contains("open_tab_group_popup"), "{src}: {err}");
        }
    }

    #[test]
    fn groups_parse_in_order_with_their_tabs_and_labels() {
        let groups =
            parse(r#"{ { tabs = { 3, 4 }, label = "p" }, { tabs = {}, label = "q" } }"#).unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].tabs, vec![3, 4]);
        assert_eq!(groups[0].label, "p");
        assert!(groups[1].tabs.is_empty());
    }

    #[test]
    fn marks_and_busy_parse_with_their_group() {
        let groups = parse(
            r#"{ { tabs = { 3, 4 }, label = "p", marks = { { tab = 4, glyph = "!" } }, busy = "o" } }"#,
        )
        .unwrap();
        assert_eq!(groups[0].marks.len(), 1);
        assert_eq!(groups[0].marks[0].tab, 4);
        assert_eq!(groups[0].marks[0].glyph, "!");
        assert_eq!(groups[0].busy.as_deref(), Some("o"));
    }

    #[test]
    fn a_mark_outside_its_group_or_without_a_glyph_is_an_error() {
        for src in [
            r#"{ { tabs = { 3, 4 }, label = "p", marks = { { tab = 9, glyph = "!" } } } }"#,
            r#"{ { tabs = { 3, 4 }, label = "p", marks = { { tab = 4 } } } }"#,
            r#"{ { tabs = { 3, 4 }, label = "p", marks = { { tab = 4, glyph = "" } } } }"#,
            r#"{ { tabs = { 3, 4 }, label = "p", marks = "!" } }"#,
            r#"{ { tabs = { 3, 4 }, label = "p", busy = 1 } }"#,
        ] {
            let err = parse(src).unwrap_err().to_string();
            assert!(err.contains("entry 1"), "{src}: {err}");
        }
    }

    #[test]
    fn an_empty_list_clears_the_groups() {
        assert!(parse("{}").unwrap().is_empty());
    }

    #[test]
    fn a_bad_entry_is_an_error_that_names_it() {
        let err = |src: &str| parse(src).unwrap_err().to_string();
        assert!(
            err(r#"{ { tabs = { 1 }, label = 3 } }"#).contains("entry 1: label must be a string")
        );
        assert!(
            err(r#"{ { tabs = { 1 }, label = "a" }, { tabs = "x", label = "b" } }"#)
                .contains("entry 2: tabs must be a list of tab ids")
        );
        assert!(
            err(r#"{ { tabs = { 1.5 }, label = "a" } }"#).contains("entry 1: tabs must be a list")
        );
        assert!(err(r#"{ 7 }"#).contains("entry 1 is not a table"));
        assert!(err(r#""groups""#).contains("expected a list of groups"));
    }
}
