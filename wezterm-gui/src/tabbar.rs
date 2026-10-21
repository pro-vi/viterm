use crate::termwindow::{PaneInformation, TabInformation, UIItem, UIItemType};
use config::{ConfigHandle, TabBarColors};
use finl_unicode::grapheme_clusters::Graphemes;
use mlua::FromLua;
use mux::tab::TabId;
use std::ops::Range;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use termwiz::cell::{unicode_column_width, Cell, CellAttributes};
use termwiz::color::{AnsiColor, ColorSpec};
use termwiz::escape::csi::Sgr;
use termwiz::escape::parser::Parser;
use termwiz::escape::{Action, ControlCode, CSI};
use termwiz::surface::SEQ_ZERO;
use termwiz_funcs::{format_as_escapes, truncate_right, FormatColor, FormatItem};
use wezterm_term::{Line, Progress};
use window::{IntegratedTitleButton, IntegratedTitleButtonAlignment, IntegratedTitleButtonStyle};

#[derive(Clone, Debug, PartialEq)]
pub struct TabBarState {
    line: Line,
    items: Vec<TabEntry>,
    /// The groups folded into one item each, indexed by
    /// `TabBarItem::TabGroup::group_idx`.
    folded: Vec<FoldedTabGroup>,
    /// When a tab shows the built-in indeterminate progress spinner, the instant
    /// at which the tab bar should be rebuilt to advance to the next frame;
    /// None when no spinner is visible.
    next_progress_frame_due: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabBarItem {
    None,
    LeftStatus,
    RightStatus,
    Tab {
        tab_idx: usize,
        active: bool,
    },
    /// A group of adjacent tabs drawn as one item; `active` when it holds
    /// the active tab.
    TabGroup {
        group_idx: usize,
        active: bool,
    },
    NewTabButton,
    WindowButton(IntegratedTitleButton),
}

/// A run of tabs a config asked to show as one item, with
/// `window:set_tab_groups`.
#[derive(Clone, Debug, PartialEq)]
pub struct TabGroupSpec {
    pub tabs: Vec<TabId>,
    pub label: String,
    /// Tabs of the group to name after the label, in order, each by its
    /// number and a glyph, such as a tab whose program waits for input.
    pub marks: Vec<TabGroupMark>,
    /// Text after the marks, such as a sign that some tab is busy.
    pub busy: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabGroupMark {
    pub tab: TabId,
    pub glyph: String,
}

/// The number the tab bar draws for the tab at `tab_index`.
pub fn tab_number(tab_index: usize, zero_based: bool) -> usize {
    if zero_based {
        tab_index
    } else {
        tab_index + 1
    }
}

/// The parts of a folded group's text: its tab numbers, its label, its marks
/// (each a tab number and a glyph) and its busy text.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupTitle {
    pub prefix: String,
    pub label: String,
    pub marks: Vec<String>,
    pub busy: Option<String>,
}

impl GroupTitle {
    /// The whole text, ` prefix label mark mark busy `.
    pub fn text(&self) -> String {
        self.compose(&self.label, self.marks.len(), true)
    }

    fn compose(&self, label: &str, marks: usize, busy: bool) -> String {
        let mut out = format!("{}{}", self.prefix, label);
        for mark in &self.marks[..marks] {
            out.push(' ');
            out.push_str(mark);
        }
        let dropped = self.marks.len() - marks;
        if dropped > 0 {
            out.push_str(&format!(" +{dropped}"));
        }
        if busy {
            if let Some(b) = &self.busy {
                out.push(' ');
                out.push_str(b);
            }
        }
        out.push(' ');
        out
    }

    /// The text in at most `cells` cells, cut so that the marks go last: the
    /// label shortens first, down to one character and an ellipsis; then the
    /// busy text goes; then marks go from the end, `+n` saying how many.
    /// The result is wider than `cells` when even the numbers, the shortest
    /// label and `+n` do not fit; a caller that must fit then cuts it.
    pub fn fit(&self, cells: usize) -> String {
        let width = |s: &str| unicode_column_width(s, None);
        let full = self.text();
        if width(&full) <= cells {
            return full;
        }
        let rest = width(&self.compose("", self.marks.len(), true));
        if rest + 2 <= cells {
            return self.compose(&shorten(&self.label, cells - rest), self.marks.len(), true);
        }
        let shortest = shorten(&self.label, 2);
        (0..=self.marks.len())
            .rev()
            .map(|marks| self.compose(&shortest, marks, false))
            .find(|text| width(text) <= cells)
            .unwrap_or_else(|| self.compose(&shortest, 0, false))
    }
}

/// `label` in at most `cells` cells, ending in an ellipsis when cut.
fn shorten(label: &str, cells: usize) -> String {
    if unicode_column_width(label, None) <= cells {
        return label.to_string();
    }
    format!("{}\u{2026}", truncate_right(label, cells.saturating_sub(1)))
}

/// A group drawn as one item: the tabs it holds, in tab order, each with the
/// title it would have in the bar.
#[derive(Clone, Debug, PartialEq)]
pub struct FoldedTabGroup {
    pub tabs: Range<usize>,
    /// The ids of the tabs in `tabs`, which name the group while tabs move.
    pub ids: Vec<TabId>,
    pub titles: Vec<Line>,
    /// The parts of the item's own text, for cutting it to a width.
    pub text: GroupTitle,
}

impl FoldedTabGroup {
    /// The group's tabs as (tab index, active, entry the tab bar's element
    /// code draws).
    pub fn entries(&self, active_tab: Option<usize>) -> Vec<(usize, bool, TabEntry)> {
        self.tabs
            .clone()
            .zip(self.titles.iter())
            .map(|(tab_idx, title)| {
                let active = Some(tab_idx) == active_tab;
                let entry = TabEntry {
                    item: TabBarItem::Tab { tab_idx, active },
                    title: title.clone(),
                    x: 0,
                    width: title.len(),
                };
                (tab_idx, active, entry)
            })
            .collect()
    }
}

/// The groups that fold, as (index into `groups`, range of tab indices), in
/// tab order. A group folds when it names two or more of the tabs in
/// `tab_ids`, the window's tabs in order, and they are contiguous. A tab an
/// earlier folded group took is not taken again; an id not in the window is
/// ignored.
pub fn folded_ranges(groups: &[TabGroupSpec], tab_ids: &[TabId]) -> Vec<(usize, Range<usize>)> {
    let mut taken = vec![false; tab_ids.len()];
    let mut out = vec![];
    for (group_idx, group) in groups.iter().enumerate() {
        let mut idxs: Vec<usize> = group
            .tabs
            .iter()
            .filter_map(|id| tab_ids.iter().position(|t| t == id))
            .filter(|&idx| !taken[idx])
            .collect();
        idxs.sort_unstable();
        idxs.dedup();
        if idxs.len() < 2 {
            continue;
        }
        let range = idxs[0]..idxs[idxs.len() - 1] + 1;
        if range.len() != idxs.len() {
            continue;
        }
        for idx in range.clone() {
            taken[idx] = true;
        }
        out.push((group_idx, range));
    }
    out.sort_by_key(|(_, range)| range.start);
    out
}

/// The parts of a folded group's text: the numbers of the first and last
/// tabs it holds, then its label, then its marks, each the number of its tab
/// and its glyph, all numbered as the tab bar numbers tabs.
fn group_title(
    tabs: &Range<usize>,
    tab_ids: &[TabId],
    spec: &TabGroupSpec,
    show_index: bool,
    zero_based: bool,
) -> GroupTitle {
    let prefix = if show_index {
        format!(
            " {}\u{2013}{}: ",
            tab_number(tabs.start, zero_based),
            tab_number(tabs.end - 1, zero_based)
        )
    } else {
        " ".to_string()
    };
    let marks = spec
        .marks
        .iter()
        .filter_map(|mark| {
            let idx = tabs.clone().find(|&idx| tab_ids[idx] == mark.tab)?;
            Some(format!("{}{}", tab_number(idx, zero_based), mark.glyph))
        })
        .collect();
    GroupTitle {
        prefix,
        label: spec.label.clone(),
        marks,
        busy: spec.busy.clone(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabEntry {
    pub item: TabBarItem,
    pub title: Line,
    x: usize,
    width: usize,
}

#[derive(Clone, Debug)]
struct TitleText {
    items: Vec<FormatItem>,
    len: usize,
    /// True when the built-in title path rendered an indeterminate spinner
    /// frame. A custom format-tab-title callback always leaves this false.
    has_indeterminate: bool,
}

fn call_format_tab_title(
    tab: &TabInformation,
    tab_info: &[TabInformation],
    pane_info: &[PaneInformation],
    config: &ConfigHandle,
    hover: bool,
    tab_max_width: usize,
) -> Option<TitleText> {
    match config::run_immediate_with_lua_config(|lua| {
        if let Some(lua) = lua {
            let tabs = lua.create_sequence_from(tab_info.iter().cloned())?;
            let panes = lua.create_sequence_from(pane_info.iter().cloned())?;

            let v = config::lua::emit_sync_callback(
                &*lua,
                (
                    "format-tab-title".to_string(),
                    (
                        tab.clone(),
                        tabs,
                        panes,
                        (**config).clone(),
                        hover,
                        tab_max_width,
                    ),
                ),
            )?;
            match &v {
                mlua::Value::Nil => Ok(None),
                mlua::Value::Table(_) => {
                    let items = <Vec<FormatItem>>::from_lua(v, &*lua)?;

                    let esc = format_as_escapes(items.clone())?;
                    let line = parse_status_text(&esc, CellAttributes::default());

                    Ok(Some(TitleText {
                        items,
                        len: line.len(),
                        has_indeterminate: false,
                    }))
                }
                _ => {
                    let s = String::from_lua(v, &*lua)?;
                    let line = parse_status_text(&s, CellAttributes::default());
                    Ok(Some(TitleText {
                        len: line.len(),
                        items: vec![FormatItem::Text(s)],
                        has_indeterminate: false,
                    }))
                }
            }
        } else {
            Ok(None)
        }
    }) {
        Ok(s) => s,
        Err(err) => {
            log::warn!("format-tab-title: {}", err);
            None
        }
    }
}

/// pct is a percentage in the range 0-100.
/// We want to map it to one of the nerdfonts:
///
/// * `md-checkbox_blank_circle_outline` (0xf0130) for an empty circle
/// * `md_circle_slice_1..=7` (0xf0a9e ..= 0xf0aa4) for a partly filled
///   circle
/// * `md_circle_slice_8` (0xf0aa5) for a filled circle
///
/// We use an empty circle for values close to 0%, a filled circle for values
/// close to 100%, and a partly filled circle for the rest (roughly evenly
/// distributed).
fn pct_to_glyph(pct: u8) -> char {
    match pct {
        0..=5 => '\u{f0130}',    // empty circle
        6..=18 => '\u{f0a9e}',   // centered at 12 (slightly smaller than 12.5)
        19..=31 => '\u{f0a9f}',  // centered at 25
        32..=43 => '\u{f0aa0}',  // centered at 37.5
        44..=56 => '\u{f0aa1}',  // half-filled circle, centered at 50
        57..=68 => '\u{f0aa2}',  // centered at 62.5
        69..=81 => '\u{f0aa3}',  // centered at 75
        82..=94 => '\u{f0aa4}',  // centered at 88 (slightly larger than 87.5)
        95..=100 => '\u{f0aa5}', // filled circle
        // Any other value is mapped to a filled circle.
        _ => '\u{f0aa5}',
    }
}

/// How long each indeterminate progress spinner frame is shown before the next
/// is due.
const INDETERMINATE_SPINNER_INTERVAL: Duration = Duration::from_millis(100);

/// Reference instant for the indeterminate spinner. The displayed frame and its
/// next-due time are both derived from the elapsed time since this instant, so
/// they stay in step no matter when a repaint happens to rebuild the tab bar.
static SPINNER_EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Renders `value` as a braille cell whose lit dots count up in the cell's
/// reading order, down the left column then down the right column. Stepping
/// `value` through 0..=255 reproduces the `dots8Bit` animation of
/// https://github.com/sindresorhus/cli-spinners without a lookup table.
fn braille_counter(value: u8) -> char {
    // Unicode braille dot bit values in reading order: dots 1, 2, 3, 7 fill the
    // left column and dots 4, 5, 6, 8 the right column.
    const DOTS: [u32; 8] = [0x01, 0x02, 0x04, 0x40, 0x08, 0x10, 0x20, 0x80];
    let mut pattern = 0u32;
    for (bit, dot) in DOTS.iter().enumerate() {
        if value & (1 << bit) != 0 {
            pattern |= dot;
        }
    }
    char::from_u32(0x2800 + pattern).expect("braille pattern is a valid codepoint")
}

/// Returns the spinner glyph to show for the current moment, advancing one
/// frame per INDETERMINATE_SPINNER_INTERVAL. `seed` offsets the starting frame
/// so that tabs busy at the same time do not animate in lock step.
fn indeterminate_spinner_glyph(seed: u64) -> char {
    let elapsed = SPINNER_EPOCH.elapsed().as_millis() as u64;
    let interval = INDETERMINATE_SPINNER_INTERVAL.as_millis() as u64;
    // braille_counter wraps at 256, matching the animation's frame count.
    braille_counter((elapsed / interval + seed) as u8)
}

/// Returns the instant at which the spinner next advances a frame, snapped to
/// the frame grid measured from SPINNER_EPOCH. Because the result falls on a
/// grid boundary rather than a fixed offset from now, repeated rebuilds within
/// one frame all return the same instant and the animation advances steadily
/// even when unrelated repaints rebuild the tab bar in between.
fn next_spinner_frame_due() -> Instant {
    let interval = INDETERMINATE_SPINNER_INTERVAL.as_nanos();
    let elapsed = SPINNER_EPOCH.elapsed().as_nanos();
    let next_frame = elapsed / interval + 1;
    *SPINNER_EPOCH + Duration::from_nanos((next_frame * interval) as u64)
}

/// Scrambles a tab id into a spinner phase offset. Tab ids are usually handed
/// out sequentially, which would leave adjacent tabs only one frame apart; the
/// splitmix64 finalizer avalanches the low bits so their spinners spread across
/// the animation instead.
fn spinner_phase(tab_id: usize) -> u64 {
    let mut z = tab_id as u64;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

fn compute_tab_title(
    tab: &TabInformation,
    tab_info: &[TabInformation],
    pane_info: &[PaneInformation],
    config: &ConfigHandle,
    hover: bool,
    tab_max_width: usize,
) -> TitleText {
    let title = call_format_tab_title(tab, tab_info, pane_info, config, hover, tab_max_width);

    match title {
        Some(title) => title,
        None => {
            let mut items = vec![];
            let mut len = 0;
            let mut has_indeterminate = false;

            if let Some(pane) = &tab.active_pane {
                let mut title = if tab.tab_title.is_empty() {
                    pane.title.clone()
                } else {
                    tab.tab_title.clone()
                };

                let classic_spacing = if config.use_fancy_tab_bar { "" } else { " " };
                if config.show_tab_index_in_tab_bar {
                    let index = format!(
                        "{classic_spacing}{}: ",
                        tab_number(tab.tab_index, config.tab_and_split_indices_are_zero_based)
                    );
                    len += unicode_column_width(&index, None);
                    items.push(FormatItem::Text(index));

                    title = format!("{}{classic_spacing}", title);
                }

                match pane.progress {
                    Progress::None => {}
                    Progress::Percentage(pct) | Progress::Error(pct) => {
                        let graphic = format!("{} ", pct_to_glyph(pct));
                        len += unicode_column_width(&graphic, None);
                        let color = if matches!(pane.progress, Progress::Percentage(_)) {
                            FormatItem::Foreground(FormatColor::AnsiColor(AnsiColor::Green))
                        } else {
                            FormatItem::Foreground(FormatColor::AnsiColor(AnsiColor::Red))
                        };
                        items.push(color);
                        items.push(FormatItem::Text(graphic));
                        items.push(FormatItem::Foreground(FormatColor::Default));
                    }
                    Progress::Indeterminate => {
                        has_indeterminate = true;
                        let graphic = format!(
                            "{} ",
                            indeterminate_spinner_glyph(spinner_phase(tab.tab_id))
                        );
                        len += unicode_column_width(&graphic, None);
                        items.push(FormatItem::Foreground(FormatColor::AnsiColor(
                            AnsiColor::Green,
                        )));
                        items.push(FormatItem::Text(graphic));
                        items.push(FormatItem::Foreground(FormatColor::Default));
                    }
                }

                // We have a preferred soft minimum on tab width to make it
                // easier to click on tab titles, but we'll still go below
                // this if there are too many tabs to fit the window at
                // this width.
                if !config.use_fancy_tab_bar {
                    while len + unicode_column_width(&title, None) < 5 {
                        title.push(' ');
                    }
                }

                len += unicode_column_width(&title, None);
                items.push(FormatItem::Text(title));
            } else {
                let title = " no pane ".to_string();
                len += unicode_column_width(&title, None);
                items.push(FormatItem::Text(title));
            };

            TitleText {
                len,
                items,
                has_indeterminate,
            }
        }
    }
}

fn is_tab_hover(mouse_x: Option<usize>, x: usize, tab_title_len: usize) -> bool {
    return mouse_x
        .map(|mouse_x| mouse_x >= x && mouse_x < x + tab_title_len)
        .unwrap_or(false);
}

impl TabBarState {
    pub fn default() -> Self {
        Self {
            line: Line::with_width(1, SEQ_ZERO),
            items: vec![TabEntry {
                item: TabBarItem::None,
                title: Line::from_text(" ", &CellAttributes::blank(), 1, None),
                x: 1,
                width: 1,
            }],
            folded: vec![],
            next_progress_frame_due: None,
        }
    }

    pub fn folded(&self) -> &[FoldedTabGroup] {
        &self.folded
    }

    /// The index of the folded group holding tab `tab_id`, if one does.
    pub fn folded_group_of(&self, tab_id: TabId) -> Option<usize> {
        self.folded.iter().position(|g| g.ids.contains(&tab_id))
    }

    pub fn line(&self) -> &Line {
        &self.line
    }

    pub fn items(&self) -> &[TabEntry] {
        &self.items
    }

    pub fn next_progress_frame_due(&self) -> Option<Instant> {
        self.next_progress_frame_due
    }

    fn integrated_title_buttons(
        mouse_x: Option<usize>,
        x: &mut usize,
        config: &ConfigHandle,
        items: &mut Vec<TabEntry>,
        line: &mut Line,
        colors: &TabBarColors,
    ) {
        let default_cell = if config.use_fancy_tab_bar {
            CellAttributes::default()
        } else {
            colors.new_tab().as_cell_attributes()
        };

        let default_cell_hover = if config.use_fancy_tab_bar {
            CellAttributes::default()
        } else {
            colors.new_tab_hover().as_cell_attributes()
        };

        let window_hide =
            parse_status_text(&config.tab_bar_style.window_hide, default_cell.clone());
        let window_hide_hover = parse_status_text(
            &config.tab_bar_style.window_hide_hover,
            default_cell_hover.clone(),
        );

        let window_maximize =
            parse_status_text(&config.tab_bar_style.window_maximize, default_cell.clone());
        let window_maximize_hover = parse_status_text(
            &config.tab_bar_style.window_maximize_hover,
            default_cell_hover.clone(),
        );

        let window_close =
            parse_status_text(&config.tab_bar_style.window_close, default_cell.clone());
        let window_close_hover = parse_status_text(
            &config.tab_bar_style.window_close_hover,
            default_cell_hover.clone(),
        );

        for button in &config.integrated_title_buttons {
            use IntegratedTitleButton as Button;
            let title = match button {
                Button::Hide => {
                    let hover = is_tab_hover(mouse_x, *x, window_hide_hover.len());

                    if hover {
                        &window_hide_hover
                    } else {
                        &window_hide
                    }
                }
                Button::Maximize => {
                    let hover = is_tab_hover(mouse_x, *x, window_maximize_hover.len());

                    if hover {
                        &window_maximize_hover
                    } else {
                        &window_maximize
                    }
                }
                Button::Close => {
                    let hover = is_tab_hover(mouse_x, *x, window_close_hover.len());

                    if hover {
                        &window_close_hover
                    } else {
                        &window_close
                    }
                }
            };

            line.append_line(title.to_owned(), SEQ_ZERO);

            let width = title.len();
            items.push(TabEntry {
                item: TabBarItem::WindowButton(*button),
                title: title.to_owned(),
                x: *x,
                width,
            });

            *x += width;
        }
    }

    /// Build a new tab bar from the current state
    /// mouse_x is some if the mouse is on the same row as the tab bar.
    /// title_width is the total number of cell columns in the window.
    /// window allows access to the tabs associated with the window.
    pub fn new(
        title_width: usize,
        mouse_x: Option<usize>,
        tab_info: &[TabInformation],
        pane_info: &[PaneInformation],
        colors: Option<&TabBarColors>,
        config: &ConfigHandle,
        left_status: &str,
        right_status: &str,
        groups: &[TabGroupSpec],
    ) -> Self {
        let colors = colors.cloned().unwrap_or_else(TabBarColors::default);

        let active_cell_attrs = colors.active_tab().as_cell_attributes();
        let inactive_hover_attrs = colors.inactive_tab_hover().as_cell_attributes();
        let inactive_cell_attrs = colors.inactive_tab().as_cell_attributes();
        let new_tab_hover_attrs = colors.new_tab_hover().as_cell_attributes();
        let new_tab_attrs = colors.new_tab().as_cell_attributes();

        let new_tab = parse_status_text(
            &config.tab_bar_style.new_tab,
            if config.use_fancy_tab_bar {
                CellAttributes::default()
            } else {
                new_tab_attrs.clone()
            },
        );
        let new_tab_hover = parse_status_text(
            &config.tab_bar_style.new_tab_hover,
            if config.use_fancy_tab_bar {
                CellAttributes::default()
            } else {
                new_tab_hover_attrs.clone()
            },
        );

        let use_integrated_title_buttons = config
            .window_decorations
            .contains(window::WindowDecorations::INTEGRATED_BUTTONS);

        // We ultimately want to produce a line looking like this:
        // ` | tab1-title x | tab2-title x |  +      . - X `
        // Where the `+` sign will spawn a new tab (or show a context
        // menu with tab creation options) and the other three chars
        // are symbols representing minimize, maximize and close.

        let mut active_tab_no = 0;

        let tab_titles: Vec<TitleText> = if config.show_tabs_in_tab_bar {
            tab_info
                .iter()
                .map(|tab| {
                    if tab.is_active {
                        active_tab_no = tab.tab_index;
                    }
                    compute_tab_title(
                        tab,
                        tab_info,
                        pane_info,
                        config,
                        false,
                        config.tab_max_width,
                    )
                })
                .collect()
        } else {
            vec![]
        };
        let titles_len: usize = tab_titles.iter().map(|s| s.len).sum();
        let number_of_tabs = tab_titles.len();

        let available_cells =
            title_width.saturating_sub(number_of_tabs.saturating_sub(1) + new_tab.len());
        let tab_width_max = if config.use_fancy_tab_bar || available_cells >= titles_len {
            // We can render each title with its full width
            usize::max_value()
        } else {
            // We need to clamp the length to balance them out
            available_cells / number_of_tabs
        }
        .min(config.tab_max_width);

        let mut line = Line::with_width(0, SEQ_ZERO);

        let mut x = 0;
        let mut items = vec![];
        let mut has_indeterminate_progress = false;

        let black_cell = Cell::blank_with_attrs(
            CellAttributes::default()
                .set_background(ColorSpec::TrueColor(*colors.background()))
                .clone(),
        );

        if use_integrated_title_buttons
            && config.integrated_title_button_style == IntegratedTitleButtonStyle::MacOsNative
            && config.use_fancy_tab_bar == false
            && config.tab_bar_at_bottom == false
        {
            for _ in 0..10 as usize {
                line.insert_cell(0, black_cell.clone(), title_width, SEQ_ZERO);
                x += 1;
            }
        }

        if use_integrated_title_buttons
            && config.integrated_title_button_style != IntegratedTitleButtonStyle::MacOsNative
            && config.integrated_title_button_alignment == IntegratedTitleButtonAlignment::Left
        {
            Self::integrated_title_buttons(mouse_x, &mut x, config, &mut items, &mut line, &colors);
        }

        let left_status_line = parse_status_text(left_status, black_cell.attrs().clone());
        if left_status_line.len() > 0 {
            items.push(TabEntry {
                item: TabBarItem::LeftStatus,
                title: left_status_line.clone(),
                x,
                width: left_status_line.len(),
            });
            x += left_status_line.len();
            line.append_line(left_status_line, SEQ_ZERO);
        }

        let tab_ids: Vec<TabId> = tab_info.iter().map(|tab| tab.tab_id).collect();
        let fold = if tab_titles.is_empty() {
            vec![]
        } else {
            folded_ranges(groups, &tab_ids)
        };
        let mut folded = vec![];
        let mut fold_iter = fold.into_iter().peekable();
        let mut skip_until = 0;

        for (tab_idx, tab_title) in tab_titles.iter().enumerate() {
            if tab_idx < skip_until {
                continue;
            }
            if let Some((group_idx, range)) = fold_iter.next_if(|(_, range)| range.start == tab_idx)
            {
                // Every member's title was computed above, so format-tab-title
                // still runs for each tab; the popup lists them as drawn.
                let titles = range
                    .clone()
                    .map(|idx| {
                        let esc = format_as_escapes(tab_titles[idx].items.clone())
                            .expect("already parsed ok above");
                        parse_status_text(&esc, CellAttributes::default())
                    })
                    .collect();
                let group_text = group_title(
                    &range,
                    &tab_ids,
                    &groups[group_idx],
                    config.show_tab_index_in_tab_bar,
                    config.tab_and_split_indices_are_zero_based,
                );
                let text = group_text.text();
                let width_guess = unicode_column_width(&text, None).min(tab_width_max);
                let active = range.contains(&active_tab_no);
                let hover = !active && is_tab_hover(mouse_x, x, width_guess);
                let cell_attrs = if active {
                    &active_cell_attrs
                } else if hover {
                    &inactive_hover_attrs
                } else {
                    &inactive_cell_attrs
                };
                let base_attrs = if config.use_fancy_tab_bar {
                    CellAttributes::default()
                } else {
                    cell_attrs.clone()
                };
                let mut group_line = parse_status_text(&text, base_attrs.clone());
                let title = group_line.clone();
                if group_line.len() > tab_width_max {
                    // Cut so that the marks go last (`GroupTitle::fit`).
                    group_line = parse_status_text(&group_text.fit(tab_width_max), base_attrs);
                    if group_line.len() > tab_width_max {
                        group_line.resize(tab_width_max, SEQ_ZERO);
                    }
                }
                let width = group_line.len();
                items.push(TabEntry {
                    item: TabBarItem::TabGroup {
                        group_idx: folded.len(),
                        active,
                    },
                    title,
                    x,
                    width,
                });
                folded.push(FoldedTabGroup {
                    tabs: range.clone(),
                    ids: tab_ids[range.clone()].to_vec(),
                    titles,
                    text: group_text,
                });
                line.append_line(group_line, SEQ_ZERO);
                x += width;
                skip_until = range.end;
                continue;
            }

            let tab_title_len = tab_title.len.min(tab_width_max);
            let active = tab_idx == active_tab_no;
            let hover = !active && is_tab_hover(mouse_x, x, tab_title_len);

            // Recompute the title so that it factors in both the hover state
            // and the adjusted maximum tab width based on available space.
            let tab_title = compute_tab_title(
                &tab_info[tab_idx],
                tab_info,
                pane_info,
                config,
                hover,
                tab_title_len,
            );

            let cell_attrs = if active {
                &active_cell_attrs
            } else if hover {
                &inactive_hover_attrs
            } else {
                &inactive_cell_attrs
            };

            let tab_start_idx = x;

            has_indeterminate_progress |= tab_title.has_indeterminate;

            let esc = format_as_escapes(tab_title.items.clone()).expect("already parsed ok above");
            let mut tab_line = parse_status_text(
                &esc,
                if config.use_fancy_tab_bar {
                    CellAttributes::default()
                } else {
                    cell_attrs.clone()
                },
            );

            let title = tab_line.clone();
            if tab_line.len() > tab_width_max {
                tab_line.resize(tab_width_max, SEQ_ZERO);
            }

            let width = tab_line.len();

            items.push(TabEntry {
                item: TabBarItem::Tab { tab_idx, active },
                title,
                x: tab_start_idx,
                width,
            });

            line.append_line(tab_line, SEQ_ZERO);
            x += width;
        }

        // New tab button
        if config.show_new_tab_button_in_tab_bar {
            let hover = is_tab_hover(mouse_x, x, new_tab_hover.len());

            let new_tab_button = if hover { &new_tab_hover } else { &new_tab };

            let button_start = x;
            let width = new_tab_button.len();

            line.append_line(new_tab_button.clone(), SEQ_ZERO);

            items.push(TabEntry {
                item: TabBarItem::NewTabButton,
                title: new_tab_button.clone(),
                x: button_start,
                width,
            });

            x += width;
        }

        // Reserve place for integrated title buttons
        let title_width = if use_integrated_title_buttons
            && config.integrated_title_button_style != IntegratedTitleButtonStyle::MacOsNative
            && config.integrated_title_button_alignment == IntegratedTitleButtonAlignment::Right
        {
            let window_hide =
                parse_status_text(&config.tab_bar_style.window_hide, CellAttributes::default());
            let window_hide_hover = parse_status_text(
                &config.tab_bar_style.window_hide_hover,
                CellAttributes::default(),
            );

            let window_maximize = parse_status_text(
                &config.tab_bar_style.window_maximize,
                CellAttributes::default(),
            );
            let window_maximize_hover = parse_status_text(
                &config.tab_bar_style.window_maximize_hover,
                CellAttributes::default(),
            );
            let window_close = parse_status_text(
                &config.tab_bar_style.window_close,
                CellAttributes::default(),
            );
            let window_close_hover = parse_status_text(
                &config.tab_bar_style.window_close_hover,
                CellAttributes::default(),
            );

            let hide_len = window_hide.len().max(window_hide_hover.len());
            let maximize_len = window_maximize.len().max(window_maximize_hover.len());
            let close_len = window_close.len().max(window_close_hover.len());

            let mut width_to_reserve = 0;
            for button in &config.integrated_title_buttons {
                use IntegratedTitleButton as Button;
                let button_len = match button {
                    Button::Hide => hide_len,
                    Button::Maximize => maximize_len,
                    Button::Close => close_len,
                };
                width_to_reserve += button_len;
            }

            title_width.saturating_sub(width_to_reserve)
        } else {
            title_width
        };

        let status_space_available = title_width.saturating_sub(x);

        let mut right_status_line = parse_status_text(right_status, black_cell.attrs().clone());
        items.push(TabEntry {
            item: TabBarItem::RightStatus,
            title: right_status_line.clone(),
            x,
            width: status_space_available,
        });

        while right_status_line.len() > status_space_available {
            right_status_line.remove_cell(0, SEQ_ZERO);
        }

        line.append_line(right_status_line, SEQ_ZERO);
        while line.len() < title_width {
            line.insert_cell(x, black_cell.clone(), title_width, SEQ_ZERO);
        }

        if use_integrated_title_buttons
            && config.integrated_title_button_style != IntegratedTitleButtonStyle::MacOsNative
            && config.integrated_title_button_alignment == IntegratedTitleButtonAlignment::Right
        {
            x = title_width;
            Self::integrated_title_buttons(mouse_x, &mut x, config, &mut items, &mut line, &colors);
        }

        Self {
            line,
            items,
            folded,
            next_progress_frame_due: has_indeterminate_progress.then(next_spinner_frame_due),
        }
    }

    pub fn compute_ui_items(&self, y: usize, cell_height: usize, cell_width: usize) -> Vec<UIItem> {
        let mut items = vec![];

        for entry in self.items.iter() {
            items.push(UIItem {
                x: entry.x * cell_width,
                width: entry.width * cell_width,
                y,
                height: cell_height,
                item_type: UIItemType::TabBar(entry.item),
            });
        }

        items
    }
}

pub fn parse_status_text(text: &str, default_cell: CellAttributes) -> Line {
    let mut pen = default_cell.clone();
    let mut cells = vec![];
    let mut ignoring = false;
    let mut print_buffer = String::new();

    fn flush_print(buf: &mut String, cells: &mut Vec<Cell>, pen: &CellAttributes) {
        for g in Graphemes::new(buf.as_str()) {
            let cell = Cell::new_grapheme(g, pen.clone(), None);
            let width = cell.width();
            cells.push(cell);
            for _ in 1..width {
                // Line/Screen expect double wide graphemes to be followed by a blank in
                // the next column position, otherwise we'll render incorrectly
                cells.push(Cell::blank_with_attrs(pen.clone()));
            }
        }
        buf.clear();
    }

    let mut parser = Parser::new();
    parser.parse(text.as_bytes(), |action| {
        if ignoring {
            return;
        }
        match action {
            Action::Print(c) => print_buffer.push(c),
            Action::PrintString(s) => print_buffer.push_str(&s),
            Action::Control(c) => {
                flush_print(&mut print_buffer, &mut cells, &pen);
                match c {
                    ControlCode::CarriageReturn | ControlCode::LineFeed => {
                        ignoring = true;
                    }
                    _ => {}
                }
            }
            Action::CSI(csi) => {
                flush_print(&mut print_buffer, &mut cells, &pen);
                match csi {
                    CSI::Sgr(sgr) => match sgr {
                        Sgr::Reset => pen = default_cell.clone(),
                        Sgr::Intensity(i) => {
                            pen.set_intensity(i);
                        }
                        Sgr::Underline(u) => {
                            pen.set_underline(u);
                        }
                        Sgr::Overline(o) => {
                            pen.set_overline(o);
                        }
                        Sgr::VerticalAlign(o) => {
                            pen.set_vertical_align(o);
                        }
                        Sgr::Blink(b) => {
                            pen.set_blink(b);
                        }
                        Sgr::Italic(i) => {
                            pen.set_italic(i);
                        }
                        Sgr::Inverse(inverse) => {
                            pen.set_reverse(inverse);
                        }
                        Sgr::Invisible(invis) => {
                            pen.set_invisible(invis);
                        }
                        Sgr::StrikeThrough(strike) => {
                            pen.set_strikethrough(strike);
                        }
                        Sgr::Foreground(col) => {
                            if let ColorSpec::Default = col {
                                pen.set_foreground(default_cell.foreground());
                            } else {
                                pen.set_foreground(col);
                            }
                        }
                        Sgr::Background(col) => {
                            if let ColorSpec::Default = col {
                                pen.set_background(default_cell.background());
                            } else {
                                pen.set_background(col);
                            }
                        }
                        Sgr::UnderlineColor(col) => {
                            pen.set_underline_color(col);
                        }
                        Sgr::Font(_) => {}
                    },
                    _ => {}
                }
            }
            Action::OperatingSystemCommand(_)
            | Action::DeviceControl(_)
            | Action::Esc(_)
            | Action::KittyImage(_)
            | Action::XtGetTcap(_)
            | Action::Sixel(_) => {
                flush_print(&mut print_buffer, &mut cells, &pen);
            }
        }
    });
    flush_print(&mut print_buffer, &mut cells, &pen);
    Line::from_cells(cells, SEQ_ZERO)
}

#[cfg(test)]
mod test {
    use super::*;

    fn spec(tabs: &[TabId], label: &str) -> TabGroupSpec {
        TabGroupSpec {
            tabs: tabs.to_vec(),
            label: label.to_string(),
            marks: vec![],
            busy: None,
        }
    }

    fn title(marks: &[&str], busy: Option<&str>) -> GroupTitle {
        GroupTitle {
            prefix: " 5\u{2013}10: ".to_string(),
            label: "philemon".to_string(),
            marks: marks.iter().map(|m| m.to_string()).collect(),
            busy: busy.map(str::to_string),
        }
    }

    #[test]
    fn a_tab_number_counts_from_one_unless_indices_are_zero_based() {
        assert_eq!(tab_number(4, false), 5);
        assert_eq!(tab_number(4, true), 4);
    }

    #[test]
    fn a_group_title_names_its_marks_after_its_label() {
        assert_eq!(
            title(&["7!", "6\u{2713}", "8\u{25c6}"], Some("\u{25cc}")).text(),
            " 5\u{2013}10: philemon 7! 6\u{2713} 8\u{25c6} \u{25cc} "
        );
        assert_eq!(title(&[], None).text(), " 5\u{2013}10: philemon ");
    }

    #[test]
    fn a_group_title_that_fits_is_not_cut() {
        let t = title(&["7!"], None);
        assert_eq!(t.fit(100), t.text());
    }

    #[test]
    fn a_group_title_shortens_its_label_before_any_mark() {
        let t = title(&["7!", "6\u{2713}"], Some("\u{25cc}"));
        // prefix 7 cells, " 7! 6✓ ◌ " 9 cells: 16 cells leave 4 for the label.
        assert_eq!(
            t.fit(20),
            " 5\u{2013}10: phi\u{2026} 7! 6\u{2713} \u{25cc} "
        );
    }

    #[test]
    fn a_group_title_drops_busy_then_marks_from_the_end() {
        let t = title(&["7!", "6\u{2713}", "8\u{25c6}"], Some("\u{25cc}"));
        assert_eq!(t.fit(19), " 5\u{2013}10: p\u{2026} 7! 6\u{2713} 8\u{25c6} ");
        // Two marks and "+1" need 19 cells, so 18 keeps one mark.
        assert_eq!(t.fit(18), " 5\u{2013}10: p\u{2026} 7! +2 ");
        assert_eq!(t.fit(14), " 5\u{2013}10: p\u{2026} +3 ");
    }

    #[test]
    fn a_group_title_never_cuts_its_numbers_or_a_mark() {
        let t = title(&["7!", "6\u{2713}"], None);
        let tight = t.fit(3);
        assert!(tight.starts_with(" 5\u{2013}10: "));
        assert!(tight.contains(" +2"));
    }

    #[test]
    fn a_group_numbers_its_marks_as_the_bar_numbers_tabs() {
        let mut s = spec(&[11, 12, 13], "p");
        s.marks = vec![
            TabGroupMark {
                tab: 12,
                glyph: "!".to_string(),
            },
            TabGroupMark {
                tab: 99,
                glyph: "?".to_string(),
            },
        ];
        let ids = [10, 11, 12, 13];
        assert_eq!(
            group_title(&(1..4), &ids, &s, true, false).marks,
            vec!["3!"]
        );
        assert_eq!(group_title(&(1..4), &ids, &s, true, true).marks, vec!["2!"]);
        assert_eq!(
            group_title(&(1..4), &ids, &s, true, true).prefix,
            " 1\u{2013}3: "
        );
    }

    fn tab(tab_id: TabId, tab_index: usize, is_active: bool) -> TabInformation {
        TabInformation {
            tab_id,
            tab_index,
            is_active,
            is_last_active: false,
            active_pane: None,
            window_id: 0,
            tab_title: format!("t{tab_id}"),
        }
    }

    #[test]
    fn a_group_of_adjacent_tabs_folds() {
        let ids = [10, 11, 12, 13, 14];
        assert_eq!(
            folded_ranges(&[spec(&[12, 11], "p")], &ids),
            vec![(0, 1..3)]
        );
    }

    #[test]
    fn a_group_whose_tabs_are_apart_does_not_fold() {
        let ids = [10, 11, 12, 13];
        assert_eq!(folded_ranges(&[spec(&[10, 12], "p")], &ids), vec![]);
    }

    #[test]
    fn a_group_of_one_tab_does_not_fold() {
        let ids = [10, 11];
        assert_eq!(folded_ranges(&[spec(&[11], "p")], &ids), vec![]);
        assert_eq!(folded_ranges(&[spec(&[11, 11], "p")], &ids), vec![]);
    }

    #[test]
    fn a_tab_in_two_groups_belongs_to_the_first() {
        let ids = [10, 11, 12, 13];
        let groups = [spec(&[11, 12], "a"), spec(&[12, 13], "b")];
        assert_eq!(folded_ranges(&groups, &ids), vec![(0, 1..3)]);
    }

    #[test]
    fn ids_not_in_the_window_are_ignored() {
        let ids = [10, 11, 12];
        assert_eq!(
            folded_ranges(&[spec(&[99, 11, 12], "p")], &ids),
            vec![(0, 1..3)]
        );
    }

    #[test]
    fn folded_ranges_come_back_in_tab_order() {
        let ids = [10, 11, 12, 13, 14];
        let groups = [spec(&[13, 14], "late"), spec(&[10, 11], "early")];
        assert_eq!(folded_ranges(&groups, &ids), vec![(1, 0..2), (0, 3..5)]);
    }

    #[test]
    fn a_group_reads_its_first_and_last_tab_numbers() {
        let s = spec(&[11, 12, 13], "work");
        let ids = [10, 11, 12, 13];
        let text = |show, zero| group_title(&(1..4), &ids, &s, show, zero).text();
        assert_eq!(text(true, false), " 2\u{2013}4: work ");
        assert_eq!(text(true, true), " 1\u{2013}3: work ");
        assert_eq!(text(false, false), " work ");
    }

    #[test]
    fn the_bar_draws_a_folded_group_as_one_item_in_place_of_its_tabs() {
        // format-tab-title runs through the main thread's lua state; with none,
        // every tab takes its default title.
        config::designate_this_as_the_main_thread();
        let config = ConfigHandle::default_config();
        let tabs: Vec<TabInformation> = (0..6).map(|i| tab(10 + i, i as usize, i == 2)).collect();
        let bar = TabBarState::new(
            200,
            None,
            &tabs,
            &[],
            None,
            &config,
            "",
            "",
            &[spec(&[11, 12, 13], "work")],
        );
        let kinds: Vec<TabBarItem> = bar
            .items()
            .iter()
            .map(|e| e.item)
            .filter(|i| matches!(i, TabBarItem::Tab { .. } | TabBarItem::TabGroup { .. }))
            .collect();
        assert_eq!(
            kinds,
            vec![
                TabBarItem::Tab {
                    tab_idx: 0,
                    active: false
                },
                TabBarItem::TabGroup {
                    group_idx: 0,
                    active: true
                },
                TabBarItem::Tab {
                    tab_idx: 4,
                    active: false
                },
                TabBarItem::Tab {
                    tab_idx: 5,
                    active: false
                },
            ]
        );
        let group = &bar.folded()[0];
        assert_eq!(group.tabs, 1..4);
        assert_eq!(group.titles.len(), 3);
        let text = bar
            .items()
            .iter()
            .find(|e| matches!(e.item, TabBarItem::TabGroup { .. }))
            .map(|e| e.title.as_str().to_string())
            .unwrap();
        assert_eq!(text, " 2\u{2013}4: work ");
    }

    #[test]
    fn a_group_not_folding_leaves_every_tab_in_the_bar() {
        // format-tab-title runs through the main thread's lua state; with none,
        // every tab takes its default title.
        config::designate_this_as_the_main_thread();
        let config = ConfigHandle::default_config();
        let tabs: Vec<TabInformation> = (0..4).map(|i| tab(10 + i, i as usize, false)).collect();
        let bar = TabBarState::new(
            200,
            None,
            &tabs,
            &[],
            None,
            &config,
            "",
            "",
            &[spec(&[10, 12], "apart")],
        );
        let tabs_drawn = bar
            .items()
            .iter()
            .filter(|e| matches!(e.item, TabBarItem::Tab { .. }))
            .count();
        assert_eq!(tabs_drawn, 4);
        assert!(bar.folded().is_empty());
    }
}
