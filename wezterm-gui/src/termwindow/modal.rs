use crate::termwindow::box_model::ComputedElement;
use crate::TermWindow;
use config::keyassignment::KeyAssignment;
use downcast_rs::{impl_downcast, Downcast};
use std::cell::Ref;
use wezterm_term::{KeyCode, KeyModifiers, MouseEvent};

pub trait Modal: Downcast {
    fn perform_assignment(
        &self,
        _assignment: &KeyAssignment,
        _term_window: &mut TermWindow,
    ) -> bool {
        false
    }
    fn mouse_event(&self, event: MouseEvent, term_window: &mut TermWindow) -> anyhow::Result<()>;
    fn key_down(
        &self,
        key: KeyCode,
        mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<bool>;
    /// Called for a key down that `key_down` passed on and no key
    /// assignment took. Returns true when the key should go on to the pane,
    /// as when the modal closed itself for it; by default it goes no further.
    fn key_down_unbound(
        &self,
        key: KeyCode,
        mods: KeyModifiers,
        term_window: &mut TermWindow,
    ) -> bool {
        self.key_down(key, mods, term_window).ok();
        false
    }
    fn computed_element(
        &self,
        term_window: &mut TermWindow,
    ) -> anyhow::Result<Ref<'_, [ComputedElement]>>;
    fn reconfigure(&self, term_window: &mut TermWindow);
}
impl_downcast!(Modal);
