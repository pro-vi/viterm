//! Names and times each event the window's main thread handles, so that a
//! stall can be traced to the event that caused it.
//!
//! An event's kind is its `WindowEvent` variant; a notification is named by
//! its `TermWindowNotif` variant, and a mux notification by its own variant
//! and, for an alert, the alert's. Each event's duration goes into the
//! `gui.event.<kind>` histogram. A tab bar rebuild goes into the
//! `gui.tabbar.rebuild` histogram and the `gui.tabbar.rebuild.by.<kind>`
//! counter of the event that caused it, so `periodic_stat_logging` reports
//! both. An event that takes longer than a 60 Hz frame is logged with its
//! pane, when it names one, and the rebuilds it caused.

use super::TermWindowNotif;
use mux::pane::PaneId;
use mux::MuxNotification;
use std::time::{Duration, Instant};
use window::WindowEvent;

const SLOW: Duration = Duration::from_millis(16);

#[derive(Default)]
pub struct EventLedger {
    current: Option<Current>,
}

struct Current {
    kind: String,
    pane: Option<PaneId>,
    started: Instant,
    rebuilds: u32,
    rebuild_time: Duration,
    tabs: usize,
}

impl EventLedger {
    pub fn begin(&mut self, event: &WindowEvent) {
        let (kind, pane) = event_kind(event);
        self.current = Some(Current {
            kind,
            pane,
            started: Instant::now(),
            rebuilds: 0,
            rebuild_time: Duration::ZERO,
            tabs: 0,
        });
    }

    pub fn rebuilt(&mut self, elapsed: Duration, tabs: usize) {
        metrics::histogram!("gui.tabbar.rebuild").record(elapsed);
        match self.current.as_mut() {
            Some(cur) => {
                metrics::counter!(format!("gui.tabbar.rebuild.by.{}", cur.kind)).increment(1);
                cur.rebuilds += 1;
                cur.rebuild_time += elapsed;
                cur.tabs = tabs;
            }
            None => {
                metrics::counter!("gui.tabbar.rebuild.by.no-event").increment(1);
                if elapsed >= SLOW {
                    log::info!(
                        "slow tab bar rebuild {:.1?} outside any event, {} tabs",
                        elapsed,
                        tabs
                    );
                }
            }
        }
    }

    pub fn end(&mut self) {
        let Some(cur) = self.current.take() else {
            return;
        };
        let elapsed = cur.started.elapsed();
        metrics::histogram!(format!("gui.event.{}", cur.kind)).record(elapsed);
        if elapsed < SLOW {
            return;
        }
        let pane = cur.pane.map(|p| format!(" pane {p}")).unwrap_or_default();
        if cur.rebuilds > 0 {
            log::info!(
                "slow event {:.1?}: {}{}; tab bar rebuilt {}x in {:.1?}, {} tabs",
                elapsed,
                cur.kind,
                pane,
                cur.rebuilds,
                cur.rebuild_time,
                cur.tabs
            );
        } else {
            log::info!("slow event {:.1?}: {}{}", elapsed, cur.kind, pane);
        }
    }
}

/// The variant name at the start of a `Debug` rendering, without its payload.
fn variant(debug: &str) -> &str {
    let end = debug
        .find(|c: char| c == '(' || c == ' ' || c == '{')
        .unwrap_or(debug.len());
    &debug[..end]
}

fn event_kind(event: &WindowEvent) -> (String, Option<PaneId>) {
    match event {
        WindowEvent::Notification(item) => match item.downcast_ref::<TermWindowNotif>() {
            Some(notif) => notif_kind(notif),
            None => ("Notification".to_string(), None),
        },
        other => (variant(&format!("{other:?}")).to_string(), None),
    }
}

// Braced patterns match tuple, struct and unit variants alike, so a variant
// whose fields another change reshapes still matches here.
fn notif_kind(notif: &TermWindowNotif) -> (String, Option<PaneId>) {
    let name = match notif {
        TermWindowNotif::MuxNotification(n) => return mux_kind(n),
        TermWindowNotif::PerformAssignment { pane_id, .. } => {
            return ("notif.PerformAssignment".to_string(), Some(*pane_id))
        }
        TermWindowNotif::InvalidateShapeCache { .. } => "InvalidateShapeCache",
        TermWindowNotif::SetLeftStatus { .. } => "SetLeftStatus",
        TermWindowNotif::SetRightStatus { .. } => "SetRightStatus",
        TermWindowNotif::GetDimensions { .. } => "GetDimensions",
        TermWindowNotif::GetSelectionForPane { .. } => "GetSelectionForPane",
        TermWindowNotif::GetEffectiveConfig { .. } => "GetEffectiveConfig",
        TermWindowNotif::FinishWindowEvent { .. } => "FinishWindowEvent",
        TermWindowNotif::GetConfigOverrides { .. } => "GetConfigOverrides",
        TermWindowNotif::SetConfigOverrides { .. } => "SetConfigOverrides",
        TermWindowNotif::CancelOverlayForPane { .. } => "CancelOverlayForPane",
        TermWindowNotif::CancelOverlayForTab { .. } => "CancelOverlayForTab",
        TermWindowNotif::EmitStatusUpdate { .. } => "EmitStatusUpdate",
        TermWindowNotif::Apply { .. } => "Apply",
        TermWindowNotif::SwitchToMuxWindow { .. } => "SwitchToMuxWindow",
        TermWindowNotif::SetInnerSize { .. } => "SetInnerSize",
        #[allow(unreachable_patterns)]
        _ => "other",
    };
    (format!("notif.{name}"), None)
}

fn mux_kind(n: &MuxNotification) -> (String, Option<PaneId>) {
    match n {
        MuxNotification::Alert { pane_id, alert } => (
            format!("mux.Alert.{}", variant(&format!("{alert:?}"))),
            Some(*pane_id),
        ),
        other => (format!("mux.{}", variant(&format!("{other:?}"))), None),
    }
}

#[cfg(test)]
mod test {
    use super::variant;

    #[test]
    fn variant_drops_the_payload() {
        assert_eq!(variant("NeedRepaint"), "NeedRepaint");
        assert_eq!(variant("WindowTitleChanged(\"a (b)\")"), "WindowTitleChanged");
        assert_eq!(variant("SetUserVar { name: \"x\", value: \"y\" }"), "SetUserVar");
    }
}
