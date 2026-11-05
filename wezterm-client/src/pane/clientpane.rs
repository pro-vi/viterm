use crate::domain::ClientInner;
use crate::pane::mousestate::MouseState;
use crate::pane::renderable::{hydrate_lines, RenderableInner, RenderableState};
use anyhow::bail;
use async_trait::async_trait;
use codec::*;
use config::configuration;
use config::keyassignment::ScrollbackEraseMode;
use mux::domain::DomainId;
use mux::pane::{
    alloc_pane_id, CachePolicy, CloseReason, ForEachPaneLogicalLine, LogicalLine, Pane, PaneId,
    Pattern, SearchResult, WithPaneLines,
};
use mux::renderable::{RenderableDimensions, StableCursorPosition};
use mux::tab::TabId;
use mux::{Mux, MuxNotification};
use parking_lot::{MappedMutexGuard, Mutex, MutexGuard};
use rangeset::RangeSet;
use ratelim::RateLimiter;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use termwiz::input::KeyEvent;
use termwiz::surface::SequenceNo;
use url::Url;
use wezterm_dynamic::Value;
use wezterm_term::color::ColorPalette;
use wezterm_term::{
    Alert, Clipboard, KeyCode, KeyModifiers, Line, MouseEvent, Progress, StableRowIndex,
    TerminalConfiguration, TerminalSize,
};

pub struct ClientPane {
    client: Arc<ClientInner>,
    local_pane_id: PaneId,
    pub remote_pane_id: PaneId,
    remote_tab_id: AtomicUsize,
    pub renderable: Mutex<RenderableState>,
    configured_palette: Mutex<ColorPalette>,
    palette: Mutex<ColorPalette>,
    application_palette: Mutex<bool>,
    writer: Mutex<PaneWriter>,
    mouse: Arc<Mutex<MouseState>>,
    clipboard: Mutex<Option<Arc<dyn Clipboard>>>,
    mouse_grabbed: Mutex<bool>,
    ignore_next_kill: Mutex<bool>,
    user_vars: Mutex<HashMap<String, String>>,
    config: Mutex<Option<Arc<dyn TerminalConfiguration>>>,
    unseen_output: Mutex<bool>,
    /// Set while this pane applies a focus from the server's `PaneFocused`
    /// announcement.
    applying_server_focus: Mutex<bool>,
    /// This pane's `RequestedSize`. The render dimensions are no guide: the
    /// server's render updates overwrite them, so they can hold a size the
    /// server reported after this client had already asked for another.
    requested_size: Arc<Mutex<RequestedSize>>,
    progress: Mutex<Progress>,
}

fn same_size(a: TerminalSize, b: TerminalSize) -> bool {
    a.cols == b.cols
        && a.rows == b.rows
        && a.pixel_width == b.pixel_width
        && a.pixel_height == b.pixel_height
}

/// What this client has asked the server for, what of that is still
/// unanswered, and what the server was last seen holding instead. A size
/// the server reports while a request is unanswered may predate that
/// request, so it arms nothing; the answer to a `Resize` request is what
/// marks the request applied. A report at odds with the layout arms one
/// more ask: a resync answers it at most once against that reported
/// size, until this client asks for a different size, the server reports
/// a different one, or a size is taken as the pane's own, so two clients
/// holding a pane at different sizes cannot resize each other forever.
/// The arm itself is spent only by a layout resize or by a report that
/// matches, so a pane overruled after its one answer is still put right
/// by the next resize of its tab, which needs an event of this client's
/// own and cannot take part in a loop between clients.
#[derive(Debug)]
struct RequestedSize {
    requested: TerminalSize,
    in_flight: usize,
    diverged: Option<TerminalSize>,
    reasserted: Option<(TerminalSize, TerminalSize)>,
}

impl RequestedSize {
    fn new(size: TerminalSize) -> Self {
        Self {
            requested: size,
            in_flight: 0,
            diverged: None,
            reasserted: None,
        }
    }

    fn adopt(&mut self, size: TerminalSize) {
        self.requested = size;
        self.diverged = None;
        self.reasserted = None;
    }

    /// Records a size the server reports for a pane whose own size here
    /// is `size`. A report that matches needs no answer, and the server
    /// already holds that size, so it becomes the requested one without
    /// anything being sent. A report taken while a request is unanswered
    /// is dropped: it may have been produced before the server applied
    /// that request.
    fn note_server_size(&mut self, reported: TerminalSize, size: TerminalSize) {
        if same_size(reported, size) {
            self.requested = size;
            self.diverged = None;
        } else if self.in_flight == 0 {
            self.diverged = Some(reported);
        }
    }

    /// The size to send for a pane that should be `size`, if any. This is
    /// the layout asking, so a size it already asked for is sent again
    /// when the server was seen holding another size; that ask spends
    /// the arm a report left.
    fn size_to_send(&mut self, size: TerminalSize) -> Option<TerminalSize> {
        if !same_size(size, self.requested) {
            self.requested = size;
            self.diverged = None;
            self.reasserted = None;
            self.in_flight += 1;
            return Some(size);
        }
        if self.in_flight > 0 {
            return None;
        }
        let reported = self.diverged.take()?;
        self.reasserted = Some((reported, size));
        self.in_flight += 1;
        Some(size)
    }

    /// Answers a reported size with the pane's own size: the size to
    /// send, if any. The reported size itself is never sent. One answer
    /// is given against a reported size; the arm stays, so if another
    /// size takes the pane back, a later layout resize still asks again.
    fn reconcile_server_size(
        &mut self,
        reported: TerminalSize,
        size: TerminalSize,
    ) -> Option<TerminalSize> {
        self.note_server_size(reported, size);
        if same_size(reported, size) || self.in_flight > 0 {
            return None;
        }
        if !same_size(size, self.requested) {
            return self.size_to_send(size);
        }
        if self.reasserted.map_or(false, |(against, wanted)| {
            same_size(against, reported) && same_size(wanted, size)
        }) {
            return None;
        }
        self.reasserted = Some((reported, size));
        self.in_flight += 1;
        Some(size)
    }

    fn request_completed(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }
}

impl ClientPane {
    pub fn new(
        client: &Arc<ClientInner>,
        remote_tab_id: TabId,
        remote_pane_id: PaneId,
        size: TerminalSize,
        title: &str,
    ) -> Self {
        let local_pane_id = alloc_pane_id();
        let writer = PaneWriter {
            client: Arc::clone(client),
            remote_pane_id,
        };

        let mouse = Arc::new(Mutex::new(MouseState::new(
            remote_pane_id,
            client.client.clone(),
        )));

        let fetch_limiter =
            RateLimiter::new(|config| config.ratelimit_mux_line_prefetches_per_second);

        let render = RenderableState {
            inner: RefCell::new(RenderableInner::new(
                client,
                remote_pane_id,
                local_pane_id,
                RenderableDimensions {
                    cols: size.cols as _,
                    viewport_rows: size.rows as _,
                    scrollback_rows: size.rows as _,
                    physical_top: 0,
                    scrollback_top: 0,
                    dpi: size.dpi,
                    pixel_width: size.pixel_width,
                    pixel_height: size.pixel_height,
                    reverse_video: false,
                },
                title,
                fetch_limiter,
            )),
        };

        let config = configuration();
        let palette: ColorPalette = config.resolved_palette.clone().into();

        // Advise the server of our palette preference
        promise::spawn::spawn({
            let palette = palette.clone();
            let client = Arc::clone(client);
            async move {
                client
                    .client
                    .set_configured_palette_for_pane(SetPalette {
                        pane_id: remote_pane_id,
                        palette,
                    })
                    .await
            }
        })
        .detach();

        Self {
            client: Arc::clone(client),
            mouse,
            remote_pane_id,
            local_pane_id,
            remote_tab_id: AtomicUsize::new(remote_tab_id),
            application_palette: Mutex::new(false),
            renderable: Mutex::new(render),
            writer: Mutex::new(writer),
            configured_palette: Mutex::new(palette.clone()),
            palette: Mutex::new(palette),
            clipboard: Mutex::new(None),
            mouse_grabbed: Mutex::new(false),
            ignore_next_kill: Mutex::new(false),
            unseen_output: Mutex::new(false),
            applying_server_focus: Mutex::new(false),
            requested_size: Arc::new(Mutex::new(RequestedSize::new(size))),
            user_vars: Mutex::new(HashMap::new()),
            config: Mutex::new(None),
            progress: Mutex::new(Progress::default()),
        }
    }

    /// Takes a size the server reported as this pane's size, for a pane whose
    /// tab took the server's pane tree. Nothing is sent: the server already
    /// has this size.
    pub fn adopt_server_size(&self, size: TerminalSize) {
        self.requested_size.lock().adopt(size);
        let render = self.renderable.lock();
        let mut inner = render.inner.borrow_mut();
        if inner.dimensions.cols != size.cols as usize
            || inner.dimensions.viewport_rows != size.rows as usize
            || inner.dimensions.pixel_width != size.pixel_width
            || inner.dimensions.pixel_height != size.pixel_height
        {
            inner.dimensions.cols = size.cols as usize;
            inner.dimensions.viewport_rows = size.rows as usize;
            inner.dimensions.pixel_width = size.pixel_width;
            inner.dimensions.pixel_height = size.pixel_height;
            inner.make_all_stale();
        }
    }

    /// Gives the panes of this pane's tab their layout sizes again, as a
    /// resize of the tab to its own size does. A focus made here means this
    /// client is the one in use, so a pane the server was seen holding at
    /// another client's size asks for this client's size once more; a pane
    /// the server already holds at its layout size sends nothing.
    ///
    /// Runs later on the main thread: `Tab::set_active_pane` and
    /// `Tab::set_active_idx` call `focus_changed` with the tab locked, and
    /// finding the tab locks every tab.
    fn reapply_tab_size(&self) {
        let local_pane_id = self.local_pane_id;
        promise::spawn::spawn_into_main_thread(async move {
            let tab = Mux::try_get().and_then(|mux| {
                mux.resolve_pane_id(local_pane_id)
                    .and_then(|(_, _, tab_id)| mux.get_tab(tab_id))
            });
            if let Some(tab) = tab {
                tab.resize(tab.get_size());
            }
        })
        .detach();
    }

    /// Answers a size the server reported for this pane with the size
    /// this client's layout gives it, when the two differ and no request
    /// for this pane is still unanswered. Only the layout size is sent.
    pub fn reconcile_server_size(&self, reported: TerminalSize, size: TerminalSize) {
        let to_send = self
            .requested_size
            .lock()
            .reconcile_server_size(reported, size);
        if let Some(size) = to_send {
            self.dispatch_resize(size);
        }
    }

    /// Sends a size the caller has already decided on: the render
    /// dimensions take it, and the request counts as answered when the
    /// server responds to it, whether it applied the size or answered
    /// with an error.
    fn dispatch_resize(&self, size: TerminalSize) {
        let render = self.renderable.lock();
        let mut inner = render.inner.borrow_mut();
        inner.dimensions.cols = size.cols as usize;
        inner.dimensions.viewport_rows = size.rows as usize;
        inner.dimensions.pixel_width = size.pixel_width;
        inner.dimensions.pixel_height = size.pixel_height;

        // Invalidate any cached rows on a resize
        inner.make_all_stale();

        metrics::counter!("mux.client.send.Resize").increment(1);
        if crate::domain::is_applying_pane_list() {
            metrics::counter!("mux.client.send.Resize.in_resync").increment(1);
        }

        let client = Arc::clone(&self.client);
        let requested_size = Arc::clone(&self.requested_size);
        let remote_pane_id = self.remote_pane_id;
        let remote_tab_id = self.remote_tab_id();
        promise::spawn::spawn(async move {
            let _ = client
                .client
                .resize(Resize {
                    containing_tab_id: remote_tab_id,
                    pane_id: remote_pane_id,
                    size,
                })
                .await;
            requested_size.lock().request_completed();
        })
        .detach();
        inner.update_last_send();
    }

    pub async fn process_unilateral(&self, pdu: Pdu) -> anyhow::Result<()> {
        match pdu {
            Pdu::GetPaneRenderChangesResponse(mut delta) => {
                *self.mouse_grabbed.lock() = delta.mouse_grabbed;

                let bonus_lines = std::mem::take(&mut delta.bonus_lines);
                let client = { Arc::clone(&self.renderable.lock().inner.borrow().client) };
                let bonus_lines = hydrate_lines(client, delta.pane_id, bonus_lines).await;

                self.renderable
                    .lock()
                    .inner
                    .borrow_mut()
                    .apply_changes_to_surface(delta, bonus_lines);
            }
            Pdu::SetClipboard(SetClipboard {
                clipboard,
                selection,
                ..
            }) => match self.clipboard.lock().as_ref() {
                Some(clip) => {
                    log::debug!(
                        "Pdu::SetClipboard pane={} remote={} {:?} {:?}",
                        self.local_pane_id,
                        self.remote_pane_id,
                        selection,
                        clipboard
                    );
                    clip.set_contents(selection, clipboard)?;
                }
                None => {
                    log::error!("ClientPane: Ignoring SetClipboard request {:?}", clipboard);
                }
            },
            Pdu::SetPalette(SetPalette { palette, .. }) => {
                *self.application_palette.lock() = palette != *self.configured_palette.lock();

                *self.palette.lock() = palette;
                let mux = Mux::get();
                self.renderable.lock().inner.borrow_mut().make_all_stale();
                mux.notify(MuxNotification::Alert {
                    pane_id: self.local_pane_id,
                    alert: Alert::PaletteChanged,
                });
            }
            Pdu::NotifyAlert(NotifyAlert { alert, .. }) => {
                let mux = Mux::get();
                match &alert {
                    Alert::SetUserVar { name, value } => {
                        self.user_vars.lock().insert(name.clone(), value.clone());
                    }
                    Alert::OutputSinceFocusLost => {
                        *self.unseen_output.lock() = true;
                        mux.notify(MuxNotification::Alert {
                            pane_id: self.local_pane_id,
                            alert: Alert::OutputSinceFocusLost,
                        });
                    }
                    Alert::Progress(progress) => {
                        *self.progress.lock() = progress.clone();
                        mux.notify(MuxNotification::Alert {
                            pane_id: self.local_pane_id,
                            alert: Alert::Progress(progress.clone()),
                        });
                    }
                    _ => {}
                }
                mux.notify(MuxNotification::Alert {
                    pane_id: self.local_pane_id,
                    alert,
                });
            }
            Pdu::PaneRemoved(PaneRemoved { pane_id }) => {
                log::trace!("remote pane {} has been removed", pane_id);
                self.renderable.lock().inner.borrow_mut().dead = true;
                let mux = Mux::get();
                mux.prune_dead_windows();

                self.client.expire_stale_mappings();
            }
            Pdu::PaneFocused(PaneFocused { pane_id }) => {
                // We get here whenever the pane focus is changed on the
                // server. That might be due to the user here in the GUI
                // doing things, or it may be due to a "remote"
                // `wezterm cli activate-pane-direction` or similar call
                // from some other actor.
                // The latter case is the important one: it is desirable
                // for the focus change to be reflected locally after it
                // has been changed on the server, so we work to apply
                // it here.
                log::trace!("advised of remote pane focus: {pane_id}");

                // Mark the pane focused on the server side before reconciling:
                // `focus_pane_and_containing_tab` calls `focus_changed(true)`,
                // which would otherwise echo this back as a fresh `SetFocusedPane`.
                // See <https://github.com/wezterm/wezterm/issues/4390>
                {
                    let mut focused = self.client.focused_remote_pane_id.lock().unwrap();
                    *focused = Some(pane_id);
                }

                let mux = Mux::get();
                *self.applying_server_focus.lock() = true;
                let applied = mux.focus_pane_and_containing_tab(self.local_pane_id);
                *self.applying_server_focus.lock() = false;
                if let Err(err) = applied {
                    log::error!("Error reconciling remote PaneFocused notification: {err:#}");
                }
            }
            _ => bail!("unhandled unilateral pdu: {:?}", pdu),
        };
        Ok(())
    }

    pub fn remote_pane_id(&self) -> TabId {
        self.remote_pane_id
    }

    /// The server's tab that holds this pane, which a resize or a zoom
    /// names. A pane the server moves keeps its wrapper, so a pane list
    /// sets it again.
    pub fn remote_tab_id(&self) -> TabId {
        self.remote_tab_id.load(Ordering::Relaxed)
    }

    pub(crate) fn set_remote_tab_id(&self, remote_tab_id: TabId) {
        self.remote_tab_id.store(remote_tab_id, Ordering::Relaxed);
    }

    /// Arrange to suppress the next Pane::kill call.
    /// ClientDomain uses this before it removes a pane from the local mux
    /// that the server no longer has: Mux::remove_pane calls kill, which
    /// would otherwise ask the server to kill that pane id.
    pub fn ignore_next_kill(&self) {
        *self.ignore_next_kill.lock() = true;
    }

    /// Take the working directory the server listed for this pane. Until the
    /// server pushes a render change for a pane, which it does only once the
    /// pane draws something, the client knows no directory for it; the pane
    /// listing that every attach and resync reads already carries one. A later
    /// render push replaces it as before, and a listing without one leaves the
    /// directory alone.
    pub fn seed_working_dir(&self, working_dir: Option<Url>) {
        if let Some(dir) = working_dir {
            self.renderable.lock().inner.borrow_mut().working_dir = Some(dir);
        }
    }
}

#[async_trait(?Send)]
impl Pane for ClientPane {
    fn pane_id(&self) -> PaneId {
        self.local_pane_id
    }

    fn get_metadata(&self) -> Value {
        let renderable = self.renderable.lock();
        let inner = renderable.inner.borrow();

        let mut map: BTreeMap<Value, Value> = BTreeMap::new();
        map.insert(
            Value::String("is_tardy".to_string()),
            Value::Bool(inner.is_tardy()),
        );
        map.insert(
            Value::String("since_last_response_ms".to_string()),
            Value::U64(inner.last_recv_time.elapsed().as_millis() as u64),
        );

        Value::Object(map.into())
    }

    fn get_cursor_position(&self) -> StableCursorPosition {
        self.renderable.lock().get_cursor_position()
    }

    fn get_dimensions(&self) -> RenderableDimensions {
        self.renderable.lock().get_dimensions()
    }

    fn with_lines_mut(&self, lines: Range<StableRowIndex>, with_lines: &mut dyn WithPaneLines) {
        mux::pane::impl_with_lines_via_get_lines(self, lines, with_lines);
    }

    fn for_each_logical_line_in_stable_range_mut(
        &self,
        lines: Range<StableRowIndex>,
        for_line: &mut dyn ForEachPaneLogicalLine,
    ) {
        mux::pane::impl_for_each_logical_line_via_get_logical_lines(self, lines, for_line);
    }

    fn get_lines(&self, lines: Range<StableRowIndex>) -> (StableRowIndex, Vec<Line>) {
        self.renderable.lock().get_lines(lines)
    }

    fn get_logical_lines(&self, lines: Range<StableRowIndex>) -> Vec<LogicalLine> {
        mux::pane::impl_get_logical_lines_via_get_lines(self, lines)
    }

    fn get_current_seqno(&self) -> SequenceNo {
        self.renderable.lock().get_current_seqno()
    }

    fn get_changed_since(
        &self,
        lines: Range<StableRowIndex>,
        seqno: SequenceNo,
    ) -> RangeSet<StableRowIndex> {
        self.renderable.lock().get_changed_since(lines, seqno)
    }

    fn set_clipboard(&self, clipboard: &Arc<dyn Clipboard>) {
        self.clipboard.lock().replace(Arc::clone(clipboard));
    }

    fn get_title(&self) -> String {
        let renderable = self.renderable.lock();
        let inner = renderable.inner.borrow();
        inner.title.clone()
    }

    fn get_progress(&self) -> Progress {
        self.progress.lock().clone()
    }

    fn send_paste(&self, text: &str) -> anyhow::Result<()> {
        let client = Arc::clone(&self.client);
        let remote_pane_id = self.remote_pane_id;
        self.renderable
            .lock()
            .inner
            .borrow_mut()
            .predict_from_paste(text);

        let data = text.to_owned();
        promise::spawn::spawn(async move {
            client
                .client
                .send_paste(SendPaste {
                    pane_id: remote_pane_id,
                    data,
                })
                .await
        })
        .detach();
        self.renderable.lock().inner.borrow_mut().update_last_send();
        Ok(())
    }

    fn reader(&self) -> anyhow::Result<Option<Box<dyn std::io::Read + Send>>> {
        Ok(None)
    }

    fn writer(&self) -> MappedMutexGuard<'_, dyn std::io::Write> {
        MutexGuard::map(self.writer.lock(), |writer| {
            let w: &mut dyn std::io::Write = writer;
            w
        })
    }

    fn set_zoomed(&self, zoomed: bool) {
        let render = self.renderable.lock();
        let mut inner = render.inner.borrow_mut();
        let client = Arc::clone(&self.client);
        let remote_pane_id = self.remote_pane_id;
        let remote_tab_id = self.remote_tab_id();
        // Invalidate any cached rows on a resize
        inner.make_all_stale();
        promise::spawn::spawn(async move {
            client
                .client
                .set_zoomed(SetPaneZoomed {
                    containing_tab_id: remote_tab_id,
                    pane_id: remote_pane_id,
                    zoomed,
                })
                .await
        })
        .detach();
        inner.update_last_send();
    }

    fn resize(&self, size: TerminalSize) -> anyhow::Result<()> {
        // Released before the render lock in `dispatch_resize` is taken.
        let to_send = self.requested_size.lock().size_to_send(size);
        if let Some(size) = to_send {
            self.dispatch_resize(size);
        }
        Ok(())
    }

    async fn search(
        &self,
        pattern: Pattern,
        range: Range<StableRowIndex>,
        limit: Option<u32>,
    ) -> anyhow::Result<Vec<SearchResult>> {
        match self
            .client
            .client
            .search_scrollback(SearchScrollbackRequest {
                pane_id: self.remote_pane_id,
                pattern,
                range,
                limit,
            })
            .await
        {
            Ok(SearchScrollbackResponse { results }) => Ok(results),
            Err(e) => Err(e),
        }
    }

    fn key_down(&self, key: KeyCode, mods: KeyModifiers) -> anyhow::Result<()> {
        let input_serial;
        {
            let renderable = self.renderable.lock();
            let mut inner = renderable.inner.borrow_mut();
            inner.input_serial = InputSerial::now();
            input_serial = inner.input_serial;
            inner.predict_from_key_event(key, mods);
        }
        let client = Arc::clone(&self.client);
        let remote_pane_id = self.remote_pane_id;
        promise::spawn::spawn(async move {
            client
                .client
                .key_down(SendKeyDown {
                    pane_id: remote_pane_id,
                    event: KeyEvent {
                        key,
                        modifiers: mods,
                    },
                    input_serial,
                })
                .await
        })
        .detach();
        self.renderable.lock().inner.borrow_mut().update_last_send();
        Ok(())
    }

    fn key_up(&self, _key: KeyCode, _mods: KeyModifiers) -> anyhow::Result<()> {
        // TODO: decide how to handle key_up for mux client
        Ok(())
    }

    fn kill(&self) {
        let mut ignore = self.ignore_next_kill.lock();
        if *ignore {
            *ignore = false;
            return;
        }
        let client = Arc::clone(&self.client);
        let remote_pane_id = self.remote_pane_id;
        let local_domain_id = self.client.local_domain_id;

        // We only want to ask the server to kill the pane if the user
        // explicitly requested it to die.
        // Domain detaching can implicitly call Pane::kill on the panes
        // in the domain, so we need to check here whether the domain is
        // in the detached state; if so then we must skip sending the
        // kill to the server.
        let mut send_kill = true;

        {
            let mux = Mux::get();
            if let Some(client_domain) = mux.get_domain(local_domain_id) {
                if client_domain.state() == mux::domain::DomainState::Detached {
                    send_kill = false;
                }
            }
        }

        if send_kill {
            promise::spawn::spawn(async move {
                client
                    .client
                    .kill_pane(KillPane {
                        pane_id: remote_pane_id,
                    })
                    .await
            })
            .detach();
        }
    }

    fn mouse_event(&self, event: MouseEvent) -> anyhow::Result<()> {
        self.mouse.lock().append(event);
        if MouseState::next(Arc::clone(&self.mouse)) {
            self.renderable.lock().inner.borrow_mut().update_last_send();
        }
        Ok(())
    }

    fn is_dead(&self) -> bool {
        self.renderable.lock().inner.borrow().dead
    }

    fn palette(&self) -> ColorPalette {
        self.palette.lock().clone()
    }

    fn domain_id(&self) -> DomainId {
        self.client.local_domain_id
    }

    fn is_mouse_grabbed(&self) -> bool {
        *self.mouse_grabbed.lock()
    }

    fn is_alt_screen_active(&self) -> bool {
        // FIXME: retrieve this from the remote
        false
    }

    fn get_current_working_dir(&self, _policy: CachePolicy) -> Option<Url> {
        self.renderable.lock().inner.borrow().working_dir.clone()
    }

    fn focus_changed(&self, focused: bool) {
        if focused {
            self.advise_focus();
            *self.unseen_output.lock() = false;
            // A focus applied from the server's announcement does not show
            // that this client is the one in use; a focus made here does.
            if !*self.applying_server_focus.lock() {
                self.reapply_tab_size();
            }
        }
    }

    fn erase_scrollback(&self, erase_mode: ScrollbackEraseMode) {
        let client = Arc::clone(&self.client);
        let remote_pane_id = self.remote_pane_id;
        promise::spawn::spawn(async move {
            client
                .client
                .erase_scrollback(EraseScrollbackRequest {
                    pane_id: remote_pane_id,
                    erase_mode,
                })
                .await
        })
        .detach();
    }

    fn advise_focus(&self) {
        let mut focused_pane = self.client.focused_remote_pane_id.lock().unwrap();
        if *focused_pane != Some(self.remote_pane_id) {
            focused_pane.replace(self.remote_pane_id);
            let client = Arc::clone(&self.client);
            let remote_pane_id = self.remote_pane_id;
            promise::spawn::spawn(async move {
                client
                    .client
                    .set_focused_pane_id(SetFocusedPane {
                        pane_id: remote_pane_id,
                    })
                    .await
            })
            .detach();
        }
    }

    fn has_unseen_output(&self) -> bool {
        *self.unseen_output.lock()
    }

    fn can_close_without_prompting(&self, reason: CloseReason) -> bool {
        match reason {
            CloseReason::Window => true,
            CloseReason::Tab => false,
            CloseReason::Pane => false,
        }
    }

    fn copy_user_vars(&self) -> HashMap<String, String> {
        self.user_vars.lock().clone()
    }

    fn set_config(&self, config: Arc<dyn TerminalConfiguration>) {
        let palette = config.color_palette();
        // If the application running in the pane hasn't changed the
        // palette through escape sequences, speculatively adopt the
        // new palette so that it updates with the lowest latency.
        if !*self.application_palette.lock() {
            *self.palette.lock() = palette.clone();
        }
        *self.configured_palette.lock() = palette.clone();

        // and now send the color palette to the server
        let client = Arc::clone(&self.client);
        let remote_pane_id = self.remote_pane_id;
        promise::spawn::spawn(async move {
            client
                .client
                .set_configured_palette_for_pane(SetPalette {
                    pane_id: remote_pane_id,
                    palette,
                })
                .await
        })
        .detach();
        self.config.lock().replace(config);
    }

    fn get_config(&self) -> Option<Arc<dyn TerminalConfiguration>> {
        self.config.lock().clone()
    }
}

struct PaneWriter {
    client: Arc<ClientInner>,
    remote_pane_id: TabId,
}

impl std::io::Write for PaneWriter {
    fn write(&mut self, data: &[u8]) -> Result<usize, std::io::Error> {
        promise::spawn::block_on(self.client.client.write_to_pane(WriteToPane {
            pane_id: self.remote_pane_id,
            data: data.to_vec(),
        }))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("{}", e)))?;
        Ok(data.len())
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn term_size(cols: usize, rows: usize) -> TerminalSize {
        TerminalSize {
            rows,
            cols,
            pixel_width: cols * 10,
            pixel_height: rows * 25,
            dpi: 96,
        }
    }

    #[test]
    fn a_size_already_asked_for_is_sent_again_after_a_different_report() {
        let own = term_size(80, 25);
        let mut size = RequestedSize::new(own);
        assert_eq!(size.size_to_send(own), None, "nothing new to ask for");
        size.note_server_size(term_size(80, 30), own);
        assert_eq!(size.size_to_send(own), Some(own), "the server holds 30");
        size.request_completed();
        assert_eq!(size.size_to_send(own), None, "already answered once");
        size.adopt(own);
        size.note_server_size(term_size(80, 30), own);
        assert_eq!(
            size.size_to_send(own),
            Some(own),
            "a size taken as the pane's own starts a new answer"
        );
    }

    #[test]
    fn a_report_from_before_the_latest_request_arms_no_resend() {
        let mut size = RequestedSize::new(term_size(80, 25));
        let latest = term_size(80, 26);
        assert_eq!(size.size_to_send(latest), Some(latest));
        // The server reports the size it held before it applied the
        // request above, which is still unanswered.
        size.note_server_size(term_size(80, 30), latest);
        assert_eq!(size.size_to_send(latest), None, "the request is unanswered");
        size.request_completed();
        assert_eq!(
            size.size_to_send(latest),
            None,
            "the report predates the request, so it is not a divergence"
        );
    }

    #[test]
    fn two_clients_at_different_sizes_settle() {
        let mut server = term_size(80, 30);
        let mut one = RequestedSize::new(term_size(80, 25));
        let mut two = RequestedSize::new(term_size(80, 30));
        let mut sent = 0;
        for _ in 0..10 {
            for (client, own) in [(&mut one, term_size(80, 25)), (&mut two, term_size(80, 30))] {
                if let Some(size) = client.reconcile_server_size(server, own) {
                    sent += 1;
                    server = size;
                    client.request_completed();
                }
            }
        }
        assert_eq!(sent, 2, "one answer from each client, then neither");
        assert_eq!(server, term_size(80, 30));
        // The client that lost is still armed, so a resize of its own
        // asks once more; a report that then matches ends it.
        assert_eq!(one.size_to_send(term_size(80, 25)), Some(term_size(80, 25)));
        one.request_completed();
        assert_eq!(
            one.reconcile_server_size(term_size(80, 25), term_size(80, 25)),
            None
        );
        assert_eq!(one.size_to_send(term_size(80, 25)), None);
    }

    #[test]
    fn a_report_that_matches_the_layout_size_is_not_sent_back() {
        let mut size = RequestedSize::new(term_size(80, 25));
        let split = term_size(39, 25);
        assert_eq!(size.reconcile_server_size(split, split), None);
        assert_eq!(
            size.size_to_send(split),
            None,
            "the server already holds the split size"
        );
    }

    #[test]
    fn a_new_layout_size_is_sent_after_a_topology_change() {
        // A pane that filled its tab is split; the server holds its
        // share at another tab size, this layout gives it a smaller one.
        let mut size = RequestedSize::new(term_size(80, 25));
        let own = term_size(39, 25);
        assert_eq!(
            size.reconcile_server_size(term_size(49, 30), own),
            Some(own),
            "the layout size, not the reported one"
        );
    }
}
