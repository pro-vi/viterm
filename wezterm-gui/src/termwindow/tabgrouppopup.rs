//! The popup a folded tab group opens: the tabs the group holds, drawn as
//! the tab bar draws tabs, floating next to the group's item.
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
use mux::tab::TabId;
use std::cell::{Ref, RefCell};
use wezterm_term::{KeyCode, KeyModifiers, MouseEvent};

pub struct TabGroupPopup {
    /// A tab of the group the popup lists. Tab indices change when tabs
    /// move; the id names the group for as long as the tab is in it.
    pub tab_id: TabId,
    elements: RefCell<Vec<ComputedElement>>,
}

impl TabGroupPopup {
    pub fn new(tab_id: TabId) -> Self {
        Self {
            tab_id,
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

    /// Escape closes the popup. Any other key goes on to the key bindings,
    /// so a binding such as ActivateTab still works while it is open.
    fn key_down(
        &self,
        key: KeyCode,
        mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<bool> {
        match (key, mods) {
            (KeyCode::Escape, KeyModifiers::NONE) => {
                term_window.cancel_modal();
                Ok(true)
            }
            _ => Ok(false),
        }
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

/// Where the popup goes, in window pixels: its top left corner, and whether
/// it sits above its item. It is centred on the item it opened from and kept
/// `margin` pixels inside the window, `gap` pixels past the item. It goes
/// toward the window edge the tab bar sits on when the bar's rows there
/// hold it, so it covers tab bar rows rather than the panes' last lines;
/// otherwise it goes the other way, as with a one-row tab bar.
pub fn popup_origin(
    item: euclid::default::Rect<f32>,
    popup: euclid::default::Size2D<f32>,
    window_width: f32,
    bar: euclid::default::Rect<f32>,
    bar_at_bottom: bool,
    gap: f32,
    margin: f32,
) -> (euclid::default::Point2D<f32>, bool) {
    let centre = item.origin.x + item.size.width / 2.;
    let max_x = (window_width - popup.width - margin).max(margin);
    let x = (centre - popup.width / 2.).clamp(margin, max_x);
    let below = item.max_y() + gap;
    let above = item.origin.y - gap - popup.height;
    let fits_below = below + popup.height <= bar.max_y();
    let fits_above = above >= bar.origin.y;
    let is_above = if bar_at_bottom {
        !fits_below
    } else {
        fits_above
    };
    let y = if is_above { above } else { below };
    (euclid::point2(x, y.max(0.)), is_above)
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
            rect(0., 900., 1440., 30.),
            true,
            8.,
            4.,
        );
        assert_eq!(o, (euclid::point2(500., 852.), true));
    }

    #[test]
    fn a_popup_under_a_top_bar_sits_below_its_item() {
        let o = popup_origin(
            rect(600., 0., 100., 30.),
            euclid::size2(300., 40.),
            1440.,
            rect(0., 0., 1440., 30.),
            false,
            8.,
            4.,
        );
        assert_eq!(o, (euclid::point2(500., 38.), false));
    }

    #[test]
    fn a_popup_near_an_edge_stays_inside_the_window() {
        let bar = rect(0., 900., 1440., 30.);
        let right = popup_origin(
            rect(1400., 900., 40., 30.),
            euclid::size2(300., 40.),
            1440.,
            bar,
            true,
            8.,
            4.,
        );
        assert_eq!(right.0.x, 1136.);
        let left = popup_origin(
            rect(0., 900., 40., 30.),
            euclid::size2(300., 40.),
            1440.,
            bar,
            true,
            8.,
            4.,
        );
        assert_eq!(left.0.x, 4.);
    }

    #[test]
    fn a_popup_wider_than_the_window_starts_at_its_margin() {
        let o = popup_origin(
            rect(100., 900., 40., 30.),
            euclid::size2(2000., 40.),
            1440.,
            rect(0., 900., 1440., 30.),
            true,
            8.,
            4.,
        );
        assert_eq!(o.0.x, 4.);
    }

    #[test]
    fn a_popup_over_a_bottom_bar_covers_the_rows_below_its_item_when_they_hold_it() {
        let bar = rect(0., 840., 1440., 90.);
        let first_row = popup_origin(
            rect(600., 840., 100., 30.),
            euclid::size2(300., 40.),
            1440.,
            bar,
            true,
            8.,
            4.,
        );
        assert_eq!(first_row, (euclid::point2(500., 878.), false));
        let last_row = popup_origin(
            rect(600., 900., 100., 30.),
            euclid::size2(300., 40.),
            1440.,
            bar,
            true,
            8.,
            4.,
        );
        assert_eq!(last_row, (euclid::point2(500., 852.), true));
    }

    #[test]
    fn a_popup_under_a_top_bar_covers_the_rows_above_its_item_when_they_hold_it() {
        let bar = rect(0., 0., 1440., 90.);
        let last_row = popup_origin(
            rect(600., 60., 100., 30.),
            euclid::size2(300., 40.),
            1440.,
            bar,
            false,
            8.,
            4.,
        );
        assert_eq!(last_row, (euclid::point2(500., 12.), true));
        let first_row = popup_origin(
            rect(600., 0., 100., 30.),
            euclid::size2(300., 40.),
            1440.,
            bar,
            false,
            8.,
            4.,
        );
        assert_eq!(first_row, (euclid::point2(500., 38.), false));
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
