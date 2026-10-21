//! The popup a folded tab group opens: the tabs the group holds, drawn as
//! the tab bar draws tabs, floating over the panes next to the group's item.
//! The fancy tab bar lays it out while it lays out the bar
//! (`build_fancy_tab_bar`), because the bar knows where the item is and how
//! the style draws a tab, and hands the layout to this modal, which only
//! draws it. The bar is painted before the modal, so a new layout is drawn in
//! the frame that made it.
use crate::tabbar::TabBarItem;
use crate::termwindow::box_model::ComputedElement;
use crate::termwindow::modal::Modal;
use crate::termwindow::UIItemType;
use crate::TermWindow;
use config::{ColorSpec, ConfigHandle};
use mux::tab::TabId;
use std::cell::{Ref, RefCell};
use termwiz::color::AnsiColor;
use wezterm_term::color::{ColorAttribute, ColorPalette};
use wezterm_term::{KeyCode, KeyModifiers, MouseEvent};
use window::color::LinearRgba;

pub struct TabGroupPopup {
    /// A tab of the group the popup lists. Tab indices change when tabs
    /// move; the id names the group for as long as the tab is in it.
    pub tab_id: TabId,
    /// The tab it was opened for, drawn marked: `window:open_tab_group_popup`
    /// opens a group for one of its tabs without activating it.
    pub marked: Option<TabId>,
    /// The tab that was active when it opened. Activating another tab, by
    /// any path, closes it.
    pub opened_on: Option<TabId>,
    elements: RefCell<Vec<ComputedElement>>,
}

impl TabGroupPopup {
    pub fn new(tab_id: TabId, marked: Option<TabId>, opened_on: Option<TabId>) -> Self {
        Self {
            tab_id,
            marked,
            opened_on,
            elements: RefCell::new(vec![]),
        }
    }

    pub fn set_elements(&self, elements: Vec<ComputedElement>) {
        *self.elements.borrow_mut() = elements;
    }
}

impl Modal for TabGroupPopup {
    /// Clicks reach the popup as UI items (`UIItemType::TabGroupPopup`).
    fn mouse_event(&self, _event: MouseEvent, _term_window: &mut TermWindow) -> anyhow::Result<()> {
        Ok(())
    }

    /// A tab's hint activates that tab and Escape closes the popup. A key
    /// with Cmd, Ctrl or Alt goes on to the key bindings with the popup
    /// open, so a binding such as ActivateTab still works. Any other key
    /// closes the popup and goes on as it would with no popup. With an
    /// overlay over the active tab, such as quick select, the popup closes
    /// and the overlay gets the key.
    fn key_down(
        &self,
        key: KeyCode,
        mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<bool> {
        if term_window.active_tab_has_overlay() {
            term_window.close_tab_group_popup();
            return Ok(false);
        }
        let tabs = term_window
            .tab_bar
            .folded_group_of(self.tab_id)
            .map(|group_idx| term_window.tab_bar.folded()[group_idx].tabs.clone());
        let count = tabs.as_ref().map_or(0, |tabs| tabs.len());
        let hints = hints(&term_window.config.quick_select_alphabet, count);
        match key_on_hints(&key, mods, &hints) {
            HintKey::Pick(k) => {
                if let Some(tabs) = tabs {
                    term_window.activate_tab((tabs.start + k) as isize).ok();
                }
                term_window.close_tab_group_popup();
                Ok(true)
            }
            HintKey::Close => {
                term_window.cancel_modal();
                Ok(true)
            }
            HintKey::Leave => {
                term_window.cancel_modal();
                Ok(false)
            }
            HintKey::Pass => Ok(false),
        }
    }

    /// A key with Cmd, Ctrl or Alt that no key assignment took closes the
    /// popup and goes on to the pane.
    fn key_down_unbound(
        &self,
        _key: KeyCode,
        _mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> bool {
        term_window.close_tab_group_popup();
        true
    }

    fn computed_element(
        &self,
        _term_window: &mut TermWindow,
    ) -> anyhow::Result<Ref<'_, [ComputedElement]>> {
        Ok(Ref::map(self.elements.borrow(), |elements| {
            elements.as_slice()
        }))
    }

    /// The layout comes from the tab bar build, so this asks for the next one.
    fn reconfigure(&self, term_window: &mut TermWindow) {
        term_window.invalidate_fancy_tab_bar();
    }
}

/// The hints for `count` items: the first `count` distinct characters of
/// `alphabet` (the `quick_select_alphabet` option), lowercased. Fewer when
/// the alphabet is shorter, so the items past its end get none.
pub fn hints(alphabet: &str, count: usize) -> Vec<char> {
    let mut out: Vec<char> = vec![];
    for c in alphabet.chars().flat_map(char::to_lowercase) {
        if out.len() == count {
            break;
        }
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// What a key press does while hints are drawn.
#[derive(Debug, PartialEq, Eq)]
pub enum HintKey {
    /// The key is item `k`'s hint.
    Pick(usize),
    /// Escape: close, and the key goes no further.
    Close,
    /// Any other key without Cmd, Ctrl or Alt: close, and the key goes on as
    /// it would with nothing open.
    Leave,
    /// A key with Cmd, Ctrl or Alt, or a modifier key on its own: stay open,
    /// and the key goes on to the key bindings.
    Pass,
}

/// How `key` with `mods` is read while hints are drawn, `hints` being the
/// character of each item, in order.
pub fn key_on_hints(key: &KeyCode, mods: KeyModifiers, hints: &[char]) -> HintKey {
    if key.is_modifier() {
        return HintKey::Pass;
    }
    if mods.intersects(KeyModifiers::SUPER | KeyModifiers::CTRL | KeyModifiers::ALT) {
        return HintKey::Pass;
    }
    match key {
        KeyCode::Escape if mods == KeyModifiers::NONE => HintKey::Close,
        KeyCode::Char(c) => match hints.iter().position(|h| h == c) {
            Some(k) => HintKey::Pick(k),
            None => HintKey::Leave,
        },
        _ => HintKey::Leave,
    }
}

/// The foreground and background a hint is drawn in: quick select's label
/// colors, with the fallback quick select uses when they are not set.
pub fn hint_attributes(config: &ConfigHandle) -> (ColorAttribute, ColorAttribute) {
    let colors = &config.resolved_palette;
    (
        colors
            .quick_select_label_fg
            .unwrap_or(ColorSpec::from(AnsiColor::Olive))
            .into(),
        colors
            .quick_select_label_bg
            .unwrap_or(ColorSpec::from(AnsiColor::Black))
            .into(),
    )
}

/// `hint_attributes`, resolved for drawing an element.
pub fn hint_colors(config: &ConfigHandle, palette: &ColorPalette) -> (LinearRgba, LinearRgba) {
    let (fg, bg) = hint_attributes(config);
    (
        palette.resolve_fg(fg).to_linear(),
        palette.resolve_bg(bg).to_linear(),
    )
}

/// What a mouse press does while a popup is open, by what it lands on.
#[derive(Debug, PartialEq, Eq)]
pub enum PressOnPopup {
    /// On the popup, or on a group's item, which toggles its popup: the
    /// press is handled as it is.
    Keep,
    /// Elsewhere in the tab bar: the popup closes, then the press does what
    /// it does with no popup.
    CloseAndAct,
    /// Anywhere else, such as a pane: the popup closes, and the press and its
    /// release go no further.
    CloseOnly,
}

pub fn press_on_popup(item: Option<&UIItemType>) -> PressOnPopup {
    match item {
        Some(UIItemType::TabGroupPopup(_))
        | Some(UIItemType::TabBar(TabBarItem::TabGroup { .. })) => PressOnPopup::Keep,
        Some(UIItemType::TabBar(_)) | Some(UIItemType::CloseTab(_)) => PressOnPopup::CloseAndAct,
        _ => PressOnPopup::CloseOnly,
    }
}

/// Where the popup's top left corner goes, in window pixels: centred on the
/// item it opened from, kept `margin` pixels inside the window, and `gap`
/// pixels past the item on the side away from the window edge the tab bar
/// sits on.
pub fn popup_origin(
    item: euclid::default::Rect<f32>,
    popup: euclid::default::Size2D<f32>,
    window_width: f32,
    bar_at_bottom: bool,
    gap: f32,
    margin: f32,
) -> euclid::default::Point2D<f32> {
    let centre = item.origin.x + item.size.width / 2.;
    let max_x = (window_width - popup.width - margin).max(margin);
    let x = (centre - popup.width / 2.).clamp(margin, max_x);
    let y = if bar_at_bottom {
        item.origin.y - gap - popup.height
    } else {
        item.origin.y + item.size.height + gap
    };
    euclid::point2(x, y.max(0.))
}

/// Where the point under (or over) the popup goes: at the item's centre,
/// but never past the popup's own corners.
pub fn point_x(
    item: euclid::default::Rect<f32>,
    popup_x: f32,
    popup_width: f32,
    point_width: f32,
    inset: f32,
) -> f32 {
    let centre = item.origin.x + item.size.width / 2.;
    let min = popup_x + inset;
    let max = (popup_x + popup_width - inset - point_width).max(min);
    (centre - point_width / 2.).clamp(min, max)
}

#[cfg(test)]
mod test {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> euclid::default::Rect<f32> {
        euclid::rect(x, y, w, h)
    }

    #[test]
    fn a_popup_over_a_bottom_bar_sits_above_its_item_centred_on_it() {
        let o = popup_origin(
            rect(600., 900., 100., 30.),
            euclid::size2(300., 40.),
            1440.,
            true,
            8.,
            4.,
        );
        assert_eq!(o, euclid::point2(500., 852.));
    }

    #[test]
    fn a_popup_under_a_top_bar_sits_below_its_item() {
        let o = popup_origin(
            rect(600., 0., 100., 30.),
            euclid::size2(300., 40.),
            1440.,
            false,
            8.,
            4.,
        );
        assert_eq!(o, euclid::point2(500., 38.));
    }

    #[test]
    fn a_popup_near_an_edge_stays_inside_the_window() {
        let right = popup_origin(
            rect(1400., 900., 40., 30.),
            euclid::size2(300., 40.),
            1440.,
            true,
            8.,
            4.,
        );
        assert_eq!(right.x, 1136.);
        let left = popup_origin(
            rect(0., 900., 40., 30.),
            euclid::size2(300., 40.),
            1440.,
            true,
            8.,
            4.,
        );
        assert_eq!(left.x, 4.);
    }

    #[test]
    fn a_popup_wider_than_the_window_starts_at_its_margin() {
        let o = popup_origin(
            rect(100., 900., 40., 30.),
            euclid::size2(2000., 40.),
            1440.,
            true,
            8.,
            4.,
        );
        assert_eq!(o.x, 4.);
    }

    #[test]
    fn a_press_on_the_popup_or_a_group_keeps_it() {
        assert_eq!(
            press_on_popup(Some(&UIItemType::TabGroupPopup(Some(3)))),
            PressOnPopup::Keep
        );
        let group = UIItemType::TabBar(TabBarItem::TabGroup {
            group_idx: 0,
            active: false,
        });
        assert_eq!(press_on_popup(Some(&group)), PressOnPopup::Keep);
    }

    #[test]
    fn a_press_elsewhere_in_the_tab_bar_closes_it_and_still_acts() {
        let tab = UIItemType::TabBar(TabBarItem::Tab {
            tab_idx: 1,
            active: false,
        });
        assert_eq!(press_on_popup(Some(&tab)), PressOnPopup::CloseAndAct);
        assert_eq!(
            press_on_popup(Some(&UIItemType::TabBar(TabBarItem::NewTabButton))),
            PressOnPopup::CloseAndAct
        );
        assert_eq!(
            press_on_popup(Some(&UIItemType::TabBar(TabBarItem::None))),
            PressOnPopup::CloseAndAct
        );
        assert_eq!(
            press_on_popup(Some(&UIItemType::CloseTab(2))),
            PressOnPopup::CloseAndAct
        );
    }

    #[test]
    fn a_press_anywhere_else_only_closes_it() {
        assert_eq!(press_on_popup(None), PressOnPopup::CloseOnly);
        assert_eq!(
            press_on_popup(Some(&UIItemType::ScrollThumb)),
            PressOnPopup::CloseOnly
        );
    }

    #[test]
    fn hints_are_the_alphabet_lowercased_one_per_item() {
        assert_eq!(hints("asdfqwer", 3), vec!['a', 's', 'd']);
        assert_eq!(hints("ASDF", 2), vec!['a', 's']);
        assert_eq!(hints("as", 5), vec!['a', 's']);
        assert_eq!(hints("aAsd", 3), vec!['a', 's', 'd']);
    }

    #[test]
    fn an_empty_alphabet_gives_no_hints() {
        assert!(hints("", 4).is_empty());
        assert_eq!(
            key_on_hints(&KeyCode::Char('a'), KeyModifiers::NONE, &hints("", 4)),
            HintKey::Leave
        );
    }

    #[test]
    fn a_hint_picks_its_item() {
        let h = hints("asd", 3);
        assert_eq!(
            key_on_hints(&KeyCode::Char('s'), KeyModifiers::NONE, &h),
            HintKey::Pick(1)
        );
        assert_eq!(
            key_on_hints(&KeyCode::Char('a'), KeyModifiers::NONE, &h),
            HintKey::Pick(0)
        );
    }

    #[test]
    fn a_letter_past_the_hints_in_force_leaves() {
        let h = hints("asdf", 3);
        assert_eq!(
            key_on_hints(&KeyCode::Char('f'), KeyModifiers::NONE, &h),
            HintKey::Leave
        );
        assert_eq!(
            key_on_hints(&KeyCode::Char('y'), KeyModifiers::NONE, &h),
            HintKey::Leave
        );
        assert_eq!(
            key_on_hints(&KeyCode::Enter, KeyModifiers::NONE, &h),
            HintKey::Leave
        );
    }

    #[test]
    fn a_capital_letter_is_not_a_hint() {
        assert_eq!(
            key_on_hints(&KeyCode::Char('S'), KeyModifiers::SHIFT, &hints("asd", 3)),
            HintKey::Leave
        );
    }

    #[test]
    fn escape_closes_and_shift_escape_leaves() {
        let h = hints("asd", 3);
        assert_eq!(
            key_on_hints(&KeyCode::Escape, KeyModifiers::NONE, &h),
            HintKey::Close
        );
        assert_eq!(
            key_on_hints(&KeyCode::Escape, KeyModifiers::SHIFT, &h),
            HintKey::Leave
        );
    }

    #[test]
    fn a_key_with_cmd_ctrl_or_alt_passes_to_the_bindings() {
        let h = hints("asd", 3);
        for mods in [KeyModifiers::SUPER, KeyModifiers::CTRL, KeyModifiers::ALT] {
            assert_eq!(key_on_hints(&KeyCode::Char('5'), mods, &h), HintKey::Pass);
            assert_eq!(key_on_hints(&KeyCode::Char('s'), mods, &h), HintKey::Pass);
        }
        assert_eq!(
            key_on_hints(
                &KeyCode::Char('F'),
                KeyModifiers::ALT | KeyModifiers::SHIFT,
                &h
            ),
            HintKey::Pass
        );
    }

    #[test]
    fn a_modifier_key_on_its_own_passes() {
        let h = hints("asd", 3);
        assert_eq!(
            key_on_hints(&KeyCode::Super, KeyModifiers::NONE, &h),
            HintKey::Pass
        );
        assert_eq!(
            key_on_hints(&KeyCode::Shift, KeyModifiers::NONE, &h),
            HintKey::Pass
        );
    }

    #[test]
    fn the_point_aims_at_the_item_but_stays_inside_the_popup() {
        assert_eq!(
            point_x(rect(600., 900., 100., 30.), 500., 300., 12., 10.),
            644.
        );
        assert_eq!(
            point_x(rect(1400., 900., 40., 30.), 1136., 300., 12., 10.),
            1414.
        );
        assert_eq!(point_x(rect(0., 900., 40., 30.), 4., 300., 12., 10.), 14.);
    }
}
