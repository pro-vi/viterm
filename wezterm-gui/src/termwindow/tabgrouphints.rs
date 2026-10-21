//! Hints on the folded tab groups: while this modal is open, the fancy tab
//! bar draws a letter where each folded group's tab numbers were and dims
//! every other tab, and pressing a group's letter opens its popup without
//! activating any tab. The modal draws nothing itself; the tab bar draws the
//! hints (`build_fancy_tab_bar`), and keys are read by `key_on_hints`, as the
//! popup reads them.
use crate::termwindow::box_model::ComputedElement;
use crate::termwindow::modal::Modal;
use crate::termwindow::tabgrouppopup::{hints, key_on_hints, HintKey};
use crate::TermWindow;
use mux::tab::TabId;
use std::cell::{Ref, RefCell};
use wezterm_term::{KeyCode, KeyModifiers, MouseEvent};

pub struct TabGroupHints {
    /// The tab that was active when the hints opened. Activating another
    /// tab, by any path, closes them.
    pub opened_on: Option<TabId>,
    elements: RefCell<Vec<ComputedElement>>,
}

impl TabGroupHints {
    pub fn new(opened_on: Option<TabId>) -> Self {
        Self {
            opened_on,
            elements: RefCell::new(vec![]),
        }
    }
}

impl Modal for TabGroupHints {
    /// A press closes the hints before the window handles it
    /// (`mouse_event_impl`).
    fn mouse_event(&self, _event: MouseEvent, _term_window: &mut TermWindow) -> anyhow::Result<()> {
        Ok(())
    }

    /// A group's hint opens that group's popup in place of the hints, and
    /// Escape closes them. A key with Cmd, Ctrl or Alt goes on to the key
    /// bindings with the hints still shown. Any other key closes them and
    /// goes on as it would with no hints. With an overlay over the active
    /// tab, such as quick select, the hints close and the overlay gets the
    /// key.
    fn key_down(
        &self,
        key: KeyCode,
        mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<bool> {
        if term_window.active_tab_has_overlay() {
            term_window.close_tab_group_hints();
            return Ok(false);
        }
        let count = term_window.tab_bar.folded().len();
        let hints = hints(&term_window.config.quick_select_alphabet, count);
        match key_on_hints(&key, mods, &hints) {
            HintKey::Pick(group_idx) => {
                term_window.toggle_tab_group_popup(group_idx);
                Ok(true)
            }
            HintKey::Close => {
                term_window.close_tab_group_hints();
                Ok(true)
            }
            HintKey::Leave => {
                term_window.close_tab_group_hints();
                Ok(false)
            }
            HintKey::Pass => Ok(false),
        }
    }

    /// A key with Cmd, Ctrl or Alt that no key assignment took closes the
    /// hints and goes on to the pane.
    fn key_down_unbound(
        &self,
        _key: KeyCode,
        _mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> bool {
        term_window.close_tab_group_hints();
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

    /// The hints are drawn by the tab bar, so this asks for its next build.
    fn reconfigure(&self, term_window: &mut TermWindow) {
        term_window.invalidate_fancy_tab_bar();
    }
}
