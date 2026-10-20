//! The popup a folded tab group opens: the tabs the group holds, drawn as
//! the tab bar draws tabs, floating over the panes next to the group's item.
//! The fancy tab bar lays it out while it lays out the bar
//! (`build_fancy_tab_bar`), because the bar knows where the item is and how
//! the style draws a tab; this modal only holds what was laid out.
use crate::termwindow::box_model::ComputedElement;
use crate::termwindow::modal::Modal;
use crate::TermWindow;
use mux::tab::TabId;
use std::cell::{Ref, RefCell};
use wezterm_term::{KeyCode, KeyModifiers, MouseEvent};

pub struct TabGroupPopup {
    /// A tab of the group the popup lists. Tab indices change when tabs
    /// move; the id names the group for as long as the tab is in it.
    pub tab_id: TabId,
    element: RefCell<Option<Vec<ComputedElement>>>,
}

impl TabGroupPopup {
    pub fn new(tab_id: TabId) -> Self {
        Self {
            tab_id,
            element: RefCell::new(None),
        }
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
        term_window: &mut TermWindow,
    ) -> anyhow::Result<Ref<'_, [ComputedElement]>> {
        if self.element.borrow().is_none() {
            if let Some(elements) = term_window.tab_group_popup_elements.as_ref() {
                self.element.borrow_mut().replace(elements.clone());
            }
        }
        Ok(Ref::map(self.element.borrow(), |element| {
            element.as_deref().unwrap_or(&[])
        }))
    }

    fn reconfigure(&self, _term_window: &mut TermWindow) {
        self.element.borrow_mut().take();
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
pub fn caret_x(
    item: euclid::default::Rect<f32>,
    popup_x: f32,
    popup_width: f32,
    caret_width: f32,
    inset: f32,
) -> f32 {
    let centre = item.origin.x + item.size.width / 2.;
    let min = popup_x + inset;
    let max = (popup_x + popup_width - inset - caret_width).max(min);
    (centre - caret_width / 2.).clamp(min, max)
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
    fn the_point_aims_at_the_item_but_stays_inside_the_popup() {
        assert_eq!(
            caret_x(rect(600., 900., 100., 30.), 500., 300., 12., 10.),
            644.
        );
        assert_eq!(
            caret_x(rect(1400., 900., 40., 30.), 1136., 300., 12., 10.),
            1414.
        );
        assert_eq!(caret_x(rect(0., 900., 40., 30.), 4., 300., 12., 10.), 14.);
    }
}
