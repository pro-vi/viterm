use crate::client::Client;
use crate::pane::ClientPane;
use anyhow::{anyhow, bail};
use async_trait::async_trait;
use codec::{ListPanesResponse, SpawnV2, SplitPane};
use config::keyassignment::SpawnTabDomain;
use config::{SshDomain, TlsDomainClient, UnixDomain};
use mux::connui::{ConnectionUI, ConnectionUIParams};
use mux::domain::{alloc_domain_id, Domain, DomainId, DomainState, SplitSource};
use mux::pane::{Pane, PaneId};
use mux::tab::{SplitRequest, Tab, TabId};
use mux::window::WindowId;
use mux::{Mux, MuxNotification};
use portable_pty::CommandBuilder;
use promise::spawn::spawn_into_new_thread;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use wezterm_term::TerminalSize;

/// What a client knew before it asked the server for its pane list: the
/// remote ids it had copies of, and how many tab moves had been made.
#[derive(Default)]
struct KnownRemoteIds {
    windows: HashSet<WindowId>,
    tabs: HashSet<TabId>,
    panes: HashSet<PaneId>,
    tab_moves: u64,
}

pub struct ClientInner {
    pub client: Client,
    pub local_domain_id: DomainId,
    pub local_echo_threshold_ms: Option<u64>,
    pub overlay_lag_indicator: bool,
    remote_to_local_window: Mutex<HashMap<WindowId, WindowId>>,
    remote_to_local_tab: Mutex<HashMap<TabId, TabId>>,
    remote_to_local_pane: Mutex<HashMap<PaneId, PaneId>>,
    tab_order_support: Mutex<TabOrderSupport>,
    /// How many tab moves have been made in this client's windows. A pane
    /// list asked for before the count last moved may predate a move, since
    /// the server answers one client's requests in order, so its tab order
    /// is not applied.
    tab_moves: Mutex<u64>,
    pub focused_remote_pane_id: Mutex<Option<PaneId>>,
}

/// Whether the server takes `SetWindowTabOrder`, which a client learns once
/// per connection by sending it an empty order, or from a move it refuses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabOrderSupport {
    Unknown,
    Yes,
    No,
}

impl ClientInner {
    fn remote_to_local_window(&self, remote_window_id: WindowId) -> Option<WindowId> {
        let map = self.remote_to_local_window.lock().unwrap();
        map.get(&remote_window_id).cloned()
    }

    pub(crate) fn expire_stale_mappings(&self) {
        let mux = Mux::get();

        self.remote_to_local_pane
            .lock()
            .unwrap()
            .retain(|_remote_pane_id, local_pane_id| mux.get_pane(*local_pane_id).is_some());

        self.remote_to_local_tab
            .lock()
            .unwrap()
            .retain(
                |remote_tab_id, local_tab_id| match mux.get_tab(*local_tab_id) {
                    Some(tab) => {
                        for pos in tab.iter_panes_ignoring_zoom() {
                            if pos.pane.domain_id() == self.local_domain_id {
                                return true;
                            }
                        }
                        log::trace!(
                            "expire_stale_mappings: domain: {}. will remove \
                            {remote_tab_id} -> {local_tab_id} tab mapping \
                            because tab contains no panes from this domain",
                            self.local_domain_id,
                        );
                        false
                    }
                    None => false,
                },
            );

        self.remote_to_local_window
            .lock()
            .unwrap()
            .retain(
                |_remote_window_id, local_window_id| match mux.get_window(*local_window_id) {
                    Some(w) => {
                        for tab in w.iter_tabs() {
                            for pos in tab.iter_panes_ignoring_zoom() {
                                if pos.pane.domain_id() == self.local_domain_id {
                                    return true;
                                }
                            }
                        }
                        false
                    }
                    None => false,
                },
            );
    }

    fn record_remote_to_local_window_mapping(
        &self,
        remote_window_id: WindowId,
        local_window_id: WindowId,
    ) {
        let mut map = self.remote_to_local_window.lock().unwrap();
        map.insert(remote_window_id, local_window_id);
        log::trace!(
            "record_remote_to_local_window_mapping: {} -> {}",
            remote_window_id,
            local_window_id
        );
    }

    fn local_to_remote_tab(&self, local_tab_id: TabId) -> Option<TabId> {
        let map = self.remote_to_local_tab.lock().unwrap();
        for (remote, local) in map.iter() {
            if *local == local_tab_id {
                return Some(*remote);
            }
        }
        None
    }

    /// The remote windows mapped to a local window. Usually one; two spawns
    /// into a window that has no remote window yet each get one.
    fn remote_windows_of(&self, local_window_id: WindowId) -> Vec<WindowId> {
        self.remote_to_local_window
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, local)| **local == local_window_id)
            .map(|(remote, _)| *remote)
            .collect()
    }

    /// The order of a local window's tabs, in remote ids, as one request for
    /// each of `remote_windows`, since each server window orders only its
    /// own tabs; none when the local window is gone.
    fn tab_order_requests(
        &self,
        local_window_id: WindowId,
        remote_windows: &[WindowId],
    ) -> Vec<codec::SetWindowTabOrder> {
        if remote_windows.is_empty() {
            return vec![];
        }
        let mux = Mux::get();
        let tab_ids: Vec<TabId> = match mux.get_window(local_window_id) {
            Some(window) => window
                .iter_tabs()
                .filter_map(|tab| self.local_to_remote_tab(tab.tab_id()))
                .collect(),
            None => return vec![],
        };
        remote_windows
            .iter()
            .map(|window_id| codec::SetWindowTabOrder {
                window_id: *window_id,
                tab_ids: tab_ids.clone(),
            })
            .collect()
    }

    fn tab_order_support(&self) -> TabOrderSupport {
        *self.tab_order_support.lock().unwrap()
    }

    fn set_tab_order_support(&self, support: TabOrderSupport) {
        *self.tab_order_support.lock().unwrap() = support;
    }

    /// Learn whether the server takes tab orders, unless that is known, with
    /// an empty order for a remote window this client already maps: that
    /// moves nothing on a server that takes the request. Any failure other
    /// than one saying the server predates it leaves it to the next sync.
    async fn learn_tab_order_support(&self) {
        if self.tab_order_support() != TabOrderSupport::Unknown {
            return;
        }
        let window_id = match self.remote_to_local_window.lock().unwrap().keys().next() {
            Some(window_id) => *window_id,
            None => return,
        };
        let answer = self
            .client
            .set_window_tab_order(codec::SetWindowTabOrder {
                window_id,
                tab_ids: vec![],
            })
            .await;
        match answer {
            Ok(_) => self.set_tab_order_support(TabOrderSupport::Yes),
            Err(err) => self.note_tab_order_refusal(&err),
        }
    }

    /// A server that predates `SetWindowTabOrder` answers it with an invalid
    /// PDU error; it is not sent another.
    fn note_tab_order_refusal(&self, err: &anyhow::Error) {
        if format!("{err:#}").contains("invalid PDU") {
            self.set_tab_order_support(TabOrderSupport::No);
        } else {
            log::debug!("set_window_tab_order: {err:#}");
        }
    }

    fn tab_moves(&self) -> u64 {
        *self.tab_moves.lock().unwrap()
    }

    /// Count a tab move made here. Done when the move is made, before
    /// anything is deferred, so that a pane list asked for earlier and
    /// applied in between does not put the old order back.
    fn count_tab_move(&self) {
        *self.tab_moves.lock().unwrap() += 1;
    }

    /// Send the order of a moved local window to `remote_windows`, unless
    /// the server is known to predate the request.
    async fn send_tab_orders(&self, local_window_id: WindowId, remote_windows: Vec<WindowId>) {
        if self.tab_order_support() == TabOrderSupport::No {
            return;
        }
        for request in self.tab_order_requests(local_window_id, &remote_windows) {
            if let Err(err) = self.client.set_window_tab_order(request).await {
                self.note_tab_order_refusal(&err);
            }
        }
    }

    fn local_to_remote_window(&self, local_window_id: WindowId) -> Option<WindowId> {
        let map = self.remote_to_local_window.lock().unwrap();
        for (remote, local) in map.iter() {
            if *local == local_window_id {
                return Some(*remote);
            }
        }
        None
    }

    pub fn remote_to_local_pane_id(&self, remote_pane_id: PaneId) -> Option<TabId> {
        let mut pane_map = self.remote_to_local_pane.lock().unwrap();

        if let Some(id) = pane_map.get(&remote_pane_id) {
            return Some(*id);
        }

        let mux = Mux::get();

        for pane in mux.iter_panes() {
            if pane.domain_id() != self.local_domain_id {
                continue;
            }
            if let Some(pane) = pane.downcast_ref::<ClientPane>() {
                if pane.remote_pane_id() == remote_pane_id {
                    let local_pane_id = pane.pane_id();
                    pane_map.insert(remote_pane_id, local_pane_id);
                    return Some(local_pane_id);
                }
            }
        }
        None
    }
    pub fn remove_old_pane_mapping(&self, remote_pane_id: PaneId) {
        let mut pane_map = self.remote_to_local_pane.lock().unwrap();
        pane_map.remove(&remote_pane_id);
    }

    pub fn remove_old_tab_mapping(&self, remote_tab_id: TabId) {
        let mut tab_map = self.remote_to_local_tab.lock().unwrap();
        let old = tab_map.remove(&remote_tab_id);
        log::trace!("remove_old_tab_mapping: {remote_tab_id} -> {old:?}");
    }

    fn record_remote_to_local_tab_mapping(&self, remote_tab_id: TabId, local_tab_id: TabId) {
        let mut map = self.remote_to_local_tab.lock().unwrap();
        let prior = map.insert(remote_tab_id, local_tab_id);
        log::trace!(
            "record_remote_to_local_tab_mapping: {} -> {} \
             (prior={prior:?}, domain={})",
            remote_tab_id,
            local_tab_id,
            self.local_domain_id,
        );
    }

    pub fn remote_to_local_tab_id(&self, remote_tab_id: TabId) -> Option<TabId> {
        let map = self.remote_to_local_tab.lock().unwrap();
        map.get(&remote_tab_id).copied()
    }

    /// The remote ids this client has local copies of, and how many tab
    /// moves had been made, taken before it asks the server for its pane
    /// list. An id in here that the list leaves out
    /// was removed on the server. An id the client learned of while the
    /// request was in flight, such as a tab whose spawn reply arrived first,
    /// is missing from the list only because the list is older, and is not
    /// in here.
    fn known_remote_ids(&self) -> KnownRemoteIds {
        KnownRemoteIds {
            windows: self
                .remote_to_local_window
                .lock()
                .unwrap()
                .keys()
                .copied()
                .collect(),
            tabs: self
                .remote_to_local_tab
                .lock()
                .unwrap()
                .keys()
                .copied()
                .collect(),
            panes: self
                .remote_to_local_pane
                .lock()
                .unwrap()
                .keys()
                .copied()
                .collect(),
            tab_moves: self.tab_moves(),
        }
    }

    pub fn is_local(&self) -> bool {
        self.client.is_local
    }
}

#[derive(Clone, Debug)]
pub enum ClientDomainConfig {
    Unix(UnixDomain),
    Tls(TlsDomainClient),
    Ssh(SshDomain),
}

impl ClientDomainConfig {
    pub fn name(&self) -> &str {
        match self {
            ClientDomainConfig::Unix(unix) => &unix.name,
            ClientDomainConfig::Tls(tls) => &tls.name,
            ClientDomainConfig::Ssh(ssh) => &ssh.name,
        }
    }

    pub fn local_echo_threshold_ms(&self) -> Option<u64> {
        match self {
            ClientDomainConfig::Unix(unix) => unix.local_echo_threshold_ms,
            ClientDomainConfig::Tls(tls) => tls.local_echo_threshold_ms,
            ClientDomainConfig::Ssh(ssh) => ssh.local_echo_threshold_ms,
        }
    }

    pub fn overlay_lag_indicator(&self) -> bool {
        match self {
            ClientDomainConfig::Unix(unix) => unix.overlay_lag_indicator,
            ClientDomainConfig::Tls(tls) => tls.overlay_lag_indicator,
            ClientDomainConfig::Ssh(ssh) => ssh.overlay_lag_indicator,
        }
    }

    pub fn label(&self) -> String {
        match self {
            ClientDomainConfig::Unix(unix) => format!("unix mux {}", unix.socket_path().display()),
            ClientDomainConfig::Tls(tls) => format!("TLS mux {}", tls.remote_address),
            ClientDomainConfig::Ssh(ssh) => {
                if let Some(user) = &ssh.username {
                    format!("SSH mux {}@{}", user, ssh.remote_address)
                } else {
                    format!("SSH mux {}", ssh.remote_address)
                }
            }
        }
    }

    pub fn connect_automatically(&self) -> bool {
        match self {
            ClientDomainConfig::Unix(unix) => unix.connect_automatically,
            ClientDomainConfig::Tls(tls) => tls.connect_automatically,
            ClientDomainConfig::Ssh(ssh) => ssh.connect_automatically,
        }
    }
}

impl ClientInner {
    pub fn new(
        local_domain_id: DomainId,
        client: Client,
        local_echo_threshold_ms: Option<u64>,
        overlay_lag_indicator: bool,
    ) -> Self {
        Self {
            client,
            local_domain_id,
            local_echo_threshold_ms,
            overlay_lag_indicator,
            remote_to_local_window: Mutex::new(HashMap::new()),
            remote_to_local_tab: Mutex::new(HashMap::new()),
            remote_to_local_pane: Mutex::new(HashMap::new()),
            tab_order_support: Mutex::new(TabOrderSupport::Unknown),
            tab_moves: Mutex::new(0),
            focused_remote_pane_id: Mutex::new(None),
        }
    }
}

pub struct ClientDomain {
    config: ClientDomainConfig,
    label: String,
    inner: Mutex<Option<Arc<ClientInner>>>,
    local_domain_id: DomainId,
}

async fn update_remote_workspace(
    local_domain_id: DomainId,
    pdu: codec::SetWindowWorkspace,
) -> anyhow::Result<()> {
    let inner = ClientDomain::get_client_inner_for_domain(local_domain_id)?;
    inner.client.set_window_workspace(pdu).await?;
    Ok(())
}

fn mux_notify_client_domain(local_domain_id: DomainId, notif: MuxNotification) -> bool {
    let mux = Mux::get();
    let domain = match mux.get_domain(local_domain_id) {
        Some(domain) => domain,
        None => return false,
    };
    let client_domain = match domain.downcast_ref::<ClientDomain>() {
        Some(c) => c,
        None => return false,
    };

    match notif {
        MuxNotification::ActiveWorkspaceChanged(_client_id) => {
            // TODO: advice remote host of interesting workspaces
        }
        MuxNotification::WorkspaceRenamed {
            old_workspace,
            new_workspace,
        } => {
            if let Some(inner) = client_domain.inner() {
                let workspaces = Mux::get().iter_workspaces();
                if workspaces.contains(&old_workspace) {
                    promise::spawn::spawn(async move {
                        inner
                            .client
                            .rename_workspace(codec::RenameWorkspace {
                                old_workspace,
                                new_workspace,
                            })
                            .await
                    })
                    .detach();
                }
            }
        }
        MuxNotification::WindowWorkspaceChanged(window_id) => {
            // Mux::get_window() may trigger a borrow error if called
            // immediately; defer the bulk of this work.
            // <https://github.com/wezterm/wezterm/issues/2638>
            promise::spawn::spawn_into_main_thread(async move {
                let mux = Mux::get();
                let domain = match mux.get_domain(local_domain_id) {
                    Some(domain) => domain,
                    None => return,
                };
                let domain = match domain.downcast_ref::<ClientDomain>() {
                    Some(domain) => domain,
                    None => return,
                };
                if let Some(remote_window_id) = domain.local_to_remote_window_id(window_id) {
                    if let Some(workspace) = mux
                        .get_window(window_id)
                        .map(|w| w.get_workspace().to_string())
                    {
                        promise::spawn::spawn_into_main_thread(async move {
                            let request = codec::SetWindowWorkspace {
                                window_id: remote_window_id,
                                workspace,
                            };
                            let _ = update_remote_workspace(local_domain_id, request).await;
                        })
                        .detach();
                    }
                } else {
                    log::debug!(
                        "local window id {window_id} has no known remote window \
                        id while reconciling a local WindowWorkspaceChanged event"
                    );
                }
            })
            .detach();
        }
        MuxNotification::TabTitleChanged { tab_id, title } => {
            if let Some(remote_tab_id) = client_domain.local_to_remote_tab_id(tab_id) {
                if let Some(inner) = client_domain.inner() {
                    promise::spawn::spawn(async move {
                        inner
                            .client
                            .set_tab_title(codec::TabTitleChanged {
                                tab_id: remote_tab_id,
                                title,
                            })
                            .await
                    })
                    .detach();
                }
            }
        }
        MuxNotification::WindowTitleChanged {
            window_id,
            title: _,
        } => {
            if let Some(remote_window_id) = client_domain.local_to_remote_window_id(window_id) {
                if let Some(inner) = client_domain.inner() {
                    promise::spawn::spawn_into_main_thread(async move {
                        // De-bounce the title propagation.
                        // There is a bit of a race condition with these async
                        // updates that can trigger a cycle of WindowTitleChanged
                        // PDUs being exchanged between client and server if the
                        // title is changed twice in quick succession.
                        // To avoid that, here on the client, we wait a second
                        // and then report the now-current name of the window, rather
                        // than propagating the title encoded in the MuxNotification.
                        smol::Timer::after(std::time::Duration::from_secs(1)).await;
                        if let Some(mux) = Mux::try_get() {
                            let title = mux
                                .get_window(window_id)
                                .map(|win| win.get_title().to_string());
                            if let Some(title) = title {
                                inner
                                    .client
                                    .set_window_title(codec::WindowTitleChanged {
                                        window_id: remote_window_id,
                                        title,
                                    })
                                    .await?;
                            }
                        }
                        anyhow::Result::<()>::Ok(())
                    })
                    .detach();
                }
            }
        }
        MuxNotification::WindowTabOrderChanged(window_id) => {
            if let Some(inner) = client_domain.inner() {
                let remote_windows = inner.remote_windows_of(window_id);
                if !remote_windows.is_empty() {
                    inner.count_tab_move();
                    // Defer: Mux::get_window may hit a borrow error if called
                    // now, as for WindowWorkspaceChanged above
                    promise::spawn::spawn_into_main_thread(async move {
                        inner.send_tab_orders(window_id, remote_windows).await;
                    })
                    .detach();
                }
            }
        }
        _ => {}
    }
    true
}

impl ClientDomain {
    pub fn new(config: ClientDomainConfig) -> Self {
        let local_domain_id = alloc_domain_id();
        let label = config.label();
        Mux::get().subscribe(move |notif| mux_notify_client_domain(local_domain_id, notif));
        Self {
            config,
            label,
            inner: Mutex::new(None),
            local_domain_id,
        }
    }

    fn inner(&self) -> Option<Arc<ClientInner>> {
        self.inner.lock().unwrap().as_ref().map(Arc::clone)
    }

    pub fn connect_automatically(&self) -> bool {
        self.config.connect_automatically()
    }

    pub fn perform_detach(&self) {
        log::info!("detached domain {}", self.local_domain_id);
        self.inner.lock().unwrap().take();
        let mux = Mux::get();
        mux.domain_was_detached(self.local_domain_id);
    }

    pub fn remote_to_local_pane_id(&self, remote_pane_id: TabId) -> Option<TabId> {
        let inner = self.inner()?;
        inner.remote_to_local_pane_id(remote_pane_id)
    }

    pub fn remote_to_local_window_id(&self, remote_window_id: WindowId) -> Option<WindowId> {
        let inner = self.inner()?;
        inner.remote_to_local_window(remote_window_id)
    }

    pub fn local_to_remote_window_id(&self, local_window_id: WindowId) -> Option<WindowId> {
        let inner = self.inner()?;
        inner.local_to_remote_window(local_window_id)
    }

    pub fn local_to_remote_tab_id(&self, local_tab_id: TabId) -> Option<TabId> {
        let inner = self.inner()?;
        inner.local_to_remote_tab(local_tab_id)
    }

    pub fn get_client_inner_for_domain(domain_id: DomainId) -> anyhow::Result<Arc<ClientInner>> {
        let mux = Mux::get();
        let domain = mux
            .get_domain(domain_id)
            .ok_or_else(|| anyhow!("invalid domain id {}", domain_id))?;
        let domain = domain
            .downcast_ref::<Self>()
            .ok_or_else(|| anyhow!("domain {} is not a ClientDomain", domain_id))?;

        if let Some(inner) = domain.inner() {
            Ok(inner)
        } else {
            bail!("domain has no assigned client");
        }
    }

    /// The reader in the mux may have decided to give up on one or
    /// more tabs at the time that a disconnect was detected, and
    /// it's also possible that another client connected and adjusted
    /// the set of tabs since we were connected, so we need to re-sync.
    pub async fn reattach(domain_id: DomainId, ui: ConnectionUI) -> anyhow::Result<()> {
        let inner = Self::get_client_inner_for_domain(domain_id)?;
        // The server may have been replaced while the connection was down.
        inner.set_tab_order_support(TabOrderSupport::Unknown);
        Self::sync_with_server(inner).await?;

        ui.close();
        Ok(())
    }

    pub async fn resync(&self) -> anyhow::Result<()> {
        if let Some(inner) = self.inner() {
            Self::sync_with_server(inner).await?;
        }
        Ok(())
    }

    /// Ask the server for its pane list and make the local mux match it.
    async fn sync_with_server(inner: Arc<ClientInner>) -> anyhow::Result<()> {
        // Learned before the list is asked for, so that nothing waits
        // between the list arriving and being applied
        inner.learn_tab_order_support().await;
        let known = inner.known_remote_ids();
        let panes = inner.client.list_panes().await?;
        Self::process_pane_list(inner, panes, None, known)
    }

    pub fn process_remote_window_title_change(&self, remote_window_id: WindowId, title: String) {
        if let Some(inner) = self.inner() {
            if let Some(local_window_id) = inner.remote_to_local_window(remote_window_id) {
                if let Some(mut window) = Mux::get().get_window_mut(local_window_id) {
                    window.set_title(&title);
                }
            }
        }
    }

    pub fn process_remote_tab_title_change(&self, remote_tab_id: TabId, title: String) {
        if let Some(inner) = self.inner() {
            if let Some(local_tab_id) = inner.remote_to_local_tab_id(remote_tab_id) {
                if let Some(tab) = Mux::get().get_tab(local_tab_id) {
                    tab.set_title(&title);
                }
            }
        }
    }

    fn process_pane_list(
        inner: Arc<ClientInner>,
        panes: ListPanesResponse,
        mut primary_window_id: Option<WindowId>,
        known: KnownRemoteIds,
    ) -> anyhow::Result<()> {
        let mux = Mux::get();
        log::debug!(
            "domain {}: ListPanes result {:#?}",
            inner.local_domain_id,
            panes
        );

        // The caller "marked" the remote ids it knew of before it asked for
        // `panes`; we "Sweep" any of them that `panes` does not reference at
        // the bottom, garbage collection style
        let KnownRemoteIds {
            windows: mut remote_windows_to_forget,
            tabs: mut remote_tabs_to_forget,
            panes: mut remote_panes_to_forget,
            tab_moves,
        } = known;
        let mut tab_order: HashMap<WindowId, Vec<TabId>> = HashMap::new();

        for (tabroot, tab_title) in panes.tabs.into_iter().zip(panes.tab_titles.iter()) {
            let root_size = match tabroot.root_size() {
                Some(size) => size,
                None => continue,
            };

            if let Some((remote_window_id, remote_tab_id)) = tabroot.window_and_tab_ids() {
                let tab;

                remote_windows_to_forget.remove(&remote_window_id);
                remote_tabs_to_forget.remove(&remote_tab_id);
                tab_order
                    .entry(remote_window_id)
                    .or_default()
                    .push(remote_tab_id);

                if let Some(tab_id) = inner.remote_to_local_tab_id(remote_tab_id) {
                    match mux.get_tab(tab_id) {
                        Some(t) => tab = t,
                        None => {
                            // We likely decided that we hit EOF on the tab and
                            // removed it from the mux.  Let's add it back, but
                            // with a new id.
                            log::trace!(
                                "we had remote_to_local_tab_id mapping of \
                                 {remote_tab_id} -> {tab_id}, but the local \
                                 tab is not in the mux, make a new tab"
                            );
                            inner.remove_old_tab_mapping(remote_tab_id);
                            tab = Arc::new(Tab::new(&root_size));
                            inner.record_remote_to_local_tab_mapping(remote_tab_id, tab.tab_id());
                            mux.add_tab_no_panes(&tab);
                        }
                    };
                } else {
                    tab = Arc::new(Tab::new(&root_size));
                    mux.add_tab_no_panes(&tab);
                    inner.record_remote_to_local_tab_mapping(remote_tab_id, tab.tab_id());
                }

                tab.set_title(tab_title);

                log::debug!("domain: {} tree: {:#?}", inner.local_domain_id, tabroot);
                let mut workspace = None;
                tab.sync_with_pane_tree(root_size, tabroot, |entry| {
                    workspace.replace(entry.workspace.clone());
                    remote_panes_to_forget.remove(&entry.pane_id);
                    if let Some(pane_id) = inner.remote_to_local_pane_id(entry.pane_id) {
                        match mux.get_pane(pane_id) {
                            Some(pane) => pane,
                            None => {
                                // We likely decided that we hit EOF on the tab and
                                // removed it from the mux.  Let's add it back, but
                                // with a new id.
                                inner.remove_old_pane_mapping(entry.pane_id);
                                let pane: Arc<dyn Pane> = Arc::new(ClientPane::new(
                                    &inner,
                                    entry.tab_id,
                                    entry.pane_id,
                                    entry.size,
                                    &entry.title,
                                ));
                                mux.add_pane(&pane).expect("failed to add pane to mux");
                                pane
                            }
                        }
                    } else {
                        let pane: Arc<dyn Pane> = Arc::new(ClientPane::new(
                            &inner,
                            entry.tab_id,
                            entry.pane_id,
                            entry.size,
                            &entry.title,
                        ));
                        log::debug!(
                            "domain: {} attaching to remote pane {:?} -> local pane_id {}",
                            inner.local_domain_id,
                            entry,
                            pane.pane_id()
                        );
                        mux.add_pane(&pane).expect("failed to add pane to mux");
                        pane
                    }
                });

                if let Some(local_window_id) = inner.remote_to_local_window(remote_window_id) {
                    let mut window = mux
                        .get_window_mut(local_window_id)
                        .expect("no such window!?");
                    log::debug!(
                        "domain: {} adding tab to existing local window {}",
                        inner.local_domain_id,
                        local_window_id
                    );
                    if window.get_tab_idx_for_id(tab.tab_id()).is_none() {
                        window.push_tab(&tab);
                    }
                    continue;
                }

                if let Some(local_window_id) = primary_window_id {
                    // Verify that the workspace is consistent between the local and remote
                    // windows
                    if Some(
                        mux.get_window(local_window_id)
                            .expect("primary window to be valid")
                            .get_workspace(),
                    ) == workspace.as_deref()
                    {
                        // Yes! We can use this window
                        log::debug!(
                            "adding remote window {} as tab to local window {}",
                            remote_window_id,
                            local_window_id
                        );
                        inner.record_remote_to_local_window_mapping(
                            remote_window_id,
                            local_window_id,
                        );
                        mux.add_tab_to_window(&tab, local_window_id)?;
                        primary_window_id.take();
                        continue;
                    }
                }
                log::debug!(
                    "making new local window for remote {} in workspace {:?}",
                    remote_window_id,
                    workspace
                );
                let position = None;
                let local_window_id = mux.new_empty_window(workspace.take(), position);
                inner.record_remote_to_local_window_mapping(remote_window_id, *local_window_id);
                mux.add_tab_to_window(&tab, *local_window_id)?;
            }
        }

        // A server that takes tab orders owns them, so show its order, unless
        // this client sent a move after asking for this list, which then
        // predates it. A server that does not, or one not asked yet, keeps
        // its own order, so a move made here stays as it is. Applied without
        // WindowTabOrderChanged, which would send the order back.
        if inner.tab_order_support() == TabOrderSupport::Yes && inner.tab_moves() == tab_moves {
            for (remote_window_id, order) in tab_order {
                if let Some(local_window_id) = inner.remote_to_local_window(remote_window_id) {
                    let local: Vec<TabId> = order
                        .iter()
                        .filter_map(|remote_tab_id| inner.remote_to_local_tab_id(*remote_tab_id))
                        .collect();
                    if let Some(mut window) = mux.get_window_mut(local_window_id) {
                        window.set_tab_order(&local);
                    }
                }
            }
        }

        for (remote_window_id, window_title) in panes.window_titles {
            if let Some(local_window_id) = inner.remote_to_local_window(remote_window_id) {
                let mut window = mux
                    .get_window_mut(local_window_id)
                    .expect("no such window!?");
                window.set_title(&window_title);
            }
        }

        // "Sweep" away our mapping for ids that are no longer present in the
        // latest sync, and the local copy of each tab that is gone
        log::debug!(
            "after sync, remote_windows_to_forget={remote_windows_to_forget:?}, \
                    remote_tabs_to_forget={remote_tabs_to_forget:?}, \
                    remote_panes_to_forget={remote_panes_to_forget:?}"
        );
        if !remote_windows_to_forget.is_empty() {
            let mut windows = inner.remote_to_local_window.lock().unwrap();
            for w in remote_windows_to_forget {
                windows.remove(&w);
            }
        }
        let removed_tabs: Vec<TabId> = {
            let mut tabs = inner.remote_to_local_tab.lock().unwrap();
            remote_tabs_to_forget
                .iter()
                .filter_map(|t| tabs.remove(t))
                .collect()
        };
        for local_tab_id in removed_tabs {
            Self::remove_stale_tab(&inner, local_tab_id);
        }
        if !remote_panes_to_forget.is_empty() {
            let mut panes = inner.remote_to_local_pane.lock().unwrap();
            for p in remote_panes_to_forget {
                panes.remove(&p);
            }
        }

        Ok(())
    }

    /// The server no longer has the tab that `local_tab_id` mirrors: remove
    /// it from the local mux without asking the server to kill any of its
    /// panes. A pane the server moved into another tab is shown there now,
    /// and stays; a pane that no tab holds is gone on the server as well, and
    /// is removed here. A local window left with no tabs is then pruned,
    /// which is how the copy of a window the server removed goes away.
    fn remove_stale_tab(inner: &ClientInner, local_tab_id: TabId) {
        let mux = Mux::get();
        let tab = match mux.remove_tab_keep_panes(local_tab_id) {
            Some(tab) => tab,
            None => return,
        };
        log::debug!(
            "domain {}: removed local tab {local_tab_id}, which the server no longer has",
            inner.local_domain_id
        );
        for pos in tab.iter_panes_ignoring_zoom() {
            if let Some(pane) = pos.pane.downcast_ref::<ClientPane>() {
                if mux.resolve_pane_id(pane.pane_id()).is_none() {
                    pane.ignore_next_kill();
                    mux.remove_pane(pane.pane_id());
                }
            }
        }
    }

    fn finish_attach(
        domain_id: DomainId,
        client: Client,
        panes: ListPanesResponse,
        primary_window_id: Option<WindowId>,
    ) -> anyhow::Result<()> {
        let mux = Mux::get();
        let domain = mux
            .get_domain(domain_id)
            .ok_or_else(|| anyhow!("invalid domain id {}", domain_id))?;
        let domain = domain
            .downcast_ref::<Self>()
            .ok_or_else(|| anyhow!("domain {} is not a ClientDomain", domain_id))?;
        let threshold = domain.config.local_echo_threshold_ms();
        let overlay_lag_indicator = domain.config.overlay_lag_indicator();

        let inner = Arc::new(ClientInner::new(
            domain_id,
            client,
            threshold,
            overlay_lag_indicator,
        ));
        *domain.inner.lock().unwrap() = Some(Arc::clone(&inner));

        // `inner` is new, so there is nothing yet that `panes` could leave out
        Self::process_pane_list(inner, panes, primary_window_id, KnownRemoteIds::default())?;

        Ok(())
    }
}

#[async_trait(?Send)]
impl Domain for ClientDomain {
    fn domain_id(&self) -> DomainId {
        self.local_domain_id
    }

    fn domain_name(&self) -> &str {
        self.config.name()
    }

    async fn domain_label(&self) -> String {
        self.label.to_string()
    }

    async fn spawn_pane(
        &self,
        _size: TerminalSize,
        _command: Option<CommandBuilder>,
        _command_dir: Option<String>,
    ) -> anyhow::Result<Arc<dyn Pane>> {
        anyhow::bail!("spawn_pane not implemented for ClientDomain")
    }

    /// Forward the request to the remote; we need to translate the local ids
    /// to those that match the remote for the request, resync the changed
    /// structure, and then translate the results back to local
    async fn move_pane_to_new_tab(
        &self,
        pane_id: PaneId,
        window_id: Option<WindowId>,
        workspace_for_new_window: Option<String>,
    ) -> anyhow::Result<Option<(Arc<Tab>, WindowId)>> {
        let inner = self
            .inner()
            .ok_or_else(|| anyhow!("domain is not attached"))?;

        let local_pane = Mux::get()
            .get_pane(pane_id)
            .ok_or_else(|| anyhow!("pane_id {} is invalid", pane_id))?;
        let pane = local_pane
            .downcast_ref::<ClientPane>()
            .ok_or_else(|| anyhow!("pane_id {} is not a ClientPane", pane_id))?;

        let remote_window_id =
            window_id.and_then(|local_window| self.local_to_remote_window_id(local_window));

        let result = inner
            .client
            .move_pane_to_new_tab(codec::MovePaneToNewTab {
                pane_id: pane.remote_pane_id,
                window_id: remote_window_id,
                workspace_for_new_window,
            })
            .await?;

        self.resync().await?;

        let local_tab_id = inner
            .remote_to_local_tab_id(result.tab_id)
            .ok_or_else(|| anyhow!("remote tab {} didn't resolve after resync", result.tab_id))?;

        let local_win_id = self
            .remote_to_local_window_id(result.window_id)
            .ok_or_else(|| {
                anyhow!(
                    "remote window {} didn't resolve after resync",
                    result.window_id
                )
            })?;

        let tab = Mux::get()
            .get_tab(local_tab_id)
            .ok_or_else(|| anyhow!("local tab {local_tab_id} is invalid"))?;

        Ok(Some((tab, local_win_id)))
    }

    async fn spawn(
        &self,
        size: TerminalSize,
        command: Option<CommandBuilder>,
        command_dir: Option<String>,
        window: WindowId,
    ) -> anyhow::Result<Arc<Tab>> {
        let inner = self
            .inner()
            .ok_or_else(|| anyhow!("domain is not attached"))?;

        let workspace = Mux::get().active_workspace();

        let result = inner
            .client
            .spawn_v2(SpawnV2 {
                domain: SpawnTabDomain::DefaultDomain,
                window_id: inner.local_to_remote_window(window),
                size,
                command,
                command_dir,
                workspace,
            })
            .await?;

        inner.record_remote_to_local_window_mapping(result.window_id, window);

        let pane: Arc<dyn Pane> = Arc::new(ClientPane::new(
            &inner,
            result.tab_id,
            result.pane_id,
            size,
            "wezterm",
        ));
        let tab = Arc::new(Tab::new(&size));
        tab.assign_pane(&pane);
        inner.remove_old_tab_mapping(result.tab_id);
        inner.record_remote_to_local_tab_mapping(result.tab_id, tab.tab_id());

        let mux = Mux::get();
        mux.add_tab_and_active_pane(&tab)?;
        mux.add_tab_to_window(&tab, window)?;

        Ok(tab)
    }

    async fn split_pane(
        &self,
        source: SplitSource,
        tab_id: TabId,
        pane_id: PaneId,
        split_request: SplitRequest,
    ) -> anyhow::Result<Arc<dyn Pane>> {
        let inner = self
            .inner()
            .ok_or_else(|| anyhow!("domain is not attached"))?;

        let mux = Mux::get();

        let tab = mux
            .get_tab(tab_id)
            .ok_or_else(|| anyhow!("tab_id {} is invalid", tab_id))?;
        let local_pane = mux
            .get_pane(pane_id)
            .ok_or_else(|| anyhow!("pane_id {} is invalid", pane_id))?;
        let pane = local_pane
            .downcast_ref::<ClientPane>()
            .ok_or_else(|| anyhow!("pane_id {} is not a ClientPane", pane_id))?;

        let (command, command_dir, move_pane_id) = match source {
            SplitSource::Spawn {
                command,
                command_dir,
            } => (command, command_dir, None),
            SplitSource::MovePane(move_pane_id) => (None, None, Some(move_pane_id)),
        };

        let result = inner
            .client
            .split_pane(SplitPane {
                domain: SpawnTabDomain::CurrentPaneDomain,
                pane_id: pane.remote_pane_id,
                split_request,
                command,
                command_dir,
                move_pane_id,
            })
            .await?;

        let pane: Arc<dyn Pane> = Arc::new(ClientPane::new(
            &inner,
            result.tab_id,
            result.pane_id,
            result.size,
            "wezterm",
        ));

        let pane_index = match tab
            .iter_panes()
            .iter()
            .find(|p| p.pane.pane_id() == pane_id)
        {
            Some(p) => p.index,
            None => anyhow::bail!("invalid pane id {}", pane_id),
        };

        tab.split_and_insert(pane_index, split_request, Arc::clone(&pane))
            .ok();

        mux.add_pane(&pane)?;

        Ok(pane)
    }

    async fn attach(&self, window_id: Option<WindowId>) -> anyhow::Result<()> {
        if self.state() == DomainState::Attached {
            // Already attached
            return Ok(());
        }

        let domain_id = self.local_domain_id;
        let config = self.config.clone();

        let activity = mux::activity::Activity::new();
        let ui = ConnectionUI::with_params(ConnectionUIParams {
            window_id,
            ..Default::default()
        });
        ui.title("wezterm: Connecting...");

        ui.async_run_and_log_error({
            let ui = ui.clone();
            async move {
                let mut cloned_ui = ui.clone();
                let client = spawn_into_new_thread(move || match &config {
                    ClientDomainConfig::Unix(unix) => {
                        let initial = true;
                        let no_auto_start = false;
                        Client::new_unix_domain(
                            Some(domain_id),
                            unix,
                            initial,
                            &mut cloned_ui,
                            no_auto_start,
                        )
                    }
                    ClientDomainConfig::Tls(tls) => Client::new_tls(domain_id, tls, &mut cloned_ui),
                    ClientDomainConfig::Ssh(ssh) => Client::new_ssh(domain_id, ssh, &mut cloned_ui),
                })
                .await?;

                ui.output_str("Checking server version\n");
                client.verify_version_compat(&ui).await?;

                ui.output_str("Version check OK!  Requesting pane list...\n");
                let panes = client.list_panes().await?;
                ui.output_str(&format!(
                    "Server has {} tabs.  Attaching to local UI...\n",
                    panes.tabs.len()
                ));
                ClientDomain::finish_attach(domain_id, client, panes, window_id)?;
                // Learned now, so that a move made before the first resync is
                // not sent to a server that predates tab orders
                ClientDomain::get_client_inner_for_domain(domain_id)?
                    .learn_tab_order_support()
                    .await;
                Ok(())
            }
        })
        .await
        .map_err(|e| {
            ui.output_str(&format!("Error during attach: {:#}\n", e));
            e
        })?;

        ui.output_str("Attached!\n");
        drop(activity);
        ui.close();
        Ok(())
    }

    fn detachable(&self) -> bool {
        true
    }

    fn detach(&self) -> anyhow::Result<()> {
        self.perform_detach();
        Ok(())
    }

    fn state(&self) -> DomainState {
        if self.inner.lock().unwrap().is_some() {
            DomainState::Attached
        } else {
            DomainState::Detached
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::client::SentRequests;
    use codec::{Pdu, SetWindowTabOrder, UnitResponse};
    use mux::renderable::StableCursorPosition;
    use mux::tab::{PaneEntry, PaneNode, SplitDirection, SplitDirectionAndSize};
    use promise::spawn::SimpleExecutor;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::MutexGuard;

    /// The mux and the executor are process globals, so these tests take
    /// turns.
    static SERIAL: Mutex<()> = Mutex::new(());

    const WINDOW: WindowId = 100;
    const OTHER_WINDOW: WindowId = 101;

    /// `_serial` is last so that the next test starts only once this one's
    /// leftover tasks are dropped.
    struct Fixture {
        mux: Arc<Mux>,
        inner: Arc<ClientInner>,
        sent: SentRequests,
        executor: SimpleExecutor,
        _serial: MutexGuard<'static, ()>,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            Mux::shutdown();
            // The tasks still queued, and those that dropping the mux and the
            // client wakes, belong to this thread and must be dropped here,
            // not by the next test when it installs its own scheduler.
            promise::spawn::set_schedulers(Box::new(drop), Box::new(drop));
        }
    }

    impl Fixture {
        fn new() -> Self {
            let serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            config::use_test_configuration();
            let mux = Arc::new(Mux::new(None));
            Mux::set_mux(&mux);
            let executor = SimpleExecutor::new();
            let (client, sent) = Client::dummy();
            let inner = Arc::new(ClientInner::new(alloc_domain_id(), client, None, false));
            Self {
                mux,
                inner,
                sent,
                executor,
                _serial: serial,
            }
        }

        /// Apply a pane list as `sync_with_server` does.
        fn resync(&self, panes: ListPanesResponse) {
            self.resync_knowing(panes, self.inner.known_remote_ids());
        }

        fn resync_knowing(&self, panes: ListPanesResponse, known: KnownRemoteIds) {
            ClientDomain::process_pane_list(Arc::clone(&self.inner), panes, None, known).unwrap();
        }

        fn local_pane(&self, remote_pane_id: PaneId) -> PaneId {
            self.inner.remote_to_local_pane_id(remote_pane_id).unwrap()
        }

        fn local_tab(&self, remote_tab_id: TabId) -> TabId {
            self.inner.remote_to_local_tab_id(remote_tab_id).unwrap()
        }

        fn local_window(&self) -> WindowId {
            self.inner.remote_to_local_window(WINDOW).unwrap()
        }

        /// The local tabs of the local window, each as its local pane ids.
        fn window_tabs(&self) -> Vec<Vec<PaneId>> {
            let window = self.mux.get_window(self.local_window()).unwrap();
            window
                .iter_tabs()
                .map(|tab| {
                    tab.iter_panes_ignoring_zoom()
                        .iter()
                        .map(|pos| pos.pane.pane_id())
                        .collect()
                })
                .collect()
        }

        /// The local tabs of the local window, in order.
        fn window_tab_ids(&self) -> Vec<TabId> {
            let window = self.mux.get_window(self.local_window()).unwrap();
            window.iter_tabs().map(|tab| tab.tab_id()).collect()
        }

        /// Reorder the local window's tabs, which moving a tab in the GUI
        /// also does.
        fn move_tabs_locally(&self, order: &[TabId]) {
            let window_id = self.local_window();
            assert!(self
                .mux
                .get_window_mut(window_id)
                .unwrap()
                .set_tab_order(order));
        }

        /// Run the queued tasks. Each round runs the tasks queued before it;
        /// three rounds also run the tasks those queue, two levels down, as
        /// a kill deferred to the main thread is.
        fn run_queued(&self) {
            for _ in 0..3 {
                let done = Arc::new(AtomicBool::new(false));
                let flag = Arc::clone(&done);
                promise::spawn::spawn_into_main_thread(async move {
                    flag.store(true, Ordering::SeqCst);
                })
                .detach();
                while !done.load(Ordering::SeqCst) {
                    self.executor.tick().unwrap();
                }
            }
        }

        /// Run `task` with the server answering each tab order it sends, or
        /// refusing it with `refusal`, and return the tab orders sent.
        fn tab_orders_answered(
            &self,
            refusal: Option<&str>,
            task: impl std::future::Future<Output = ()> + 'static,
        ) -> Vec<SetWindowTabOrder> {
            self.run_queued();
            self.sent.take();
            promise::spawn::spawn(task).detach();
            self.run_queued();
            let sent = self.sent.answer(|pdu| match (pdu, refusal) {
                (Pdu::SetWindowTabOrder(_), Some(refusal)) => Err(anyhow!("{refusal}")),
                _ => Ok(Pdu::UnitResponse(UnitResponse {})),
            });
            self.run_queued();
            sent.into_iter()
                .filter_map(|pdu| match pdu {
                    Pdu::SetWindowTabOrder(request) => Some(request),
                    _ => None,
                })
                .collect()
        }

        /// The remote panes the client asked the server to kill.
        fn kills_sent(&self) -> Vec<PaneId> {
            self.run_queued();
            self.sent
                .take()
                .into_iter()
                .filter_map(|pdu| match pdu {
                    Pdu::KillPane(kill) => Some(kill.pane_id),
                    _ => None,
                })
                .collect()
        }
    }

    fn leaf(window_id: WindowId, tab_id: TabId, pane_id: PaneId, size: TerminalSize) -> PaneNode {
        PaneNode::Leaf(PaneEntry {
            window_id,
            tab_id,
            pane_id,
            title: format!("pane {pane_id}"),
            size,
            working_dir: None,
            is_active_pane: true,
            is_zoomed_pane: false,
            workspace: "default".to_string(),
            cursor_pos: StableCursorPosition::default(),
            physical_top: 0,
            top_row: 0,
            left_col: 0,
            tty_name: None,
        })
    }

    fn size(cols: usize) -> TerminalSize {
        TerminalSize {
            rows: 24,
            cols,
            ..TerminalSize::default()
        }
    }

    /// A pane list of one window holding `tabs`, each a remote tab id and the
    /// remote panes in it, side by side.
    fn pane_list(tabs: &[(TabId, &[PaneId])]) -> ListPanesResponse {
        let tabs: Vec<_> = tabs.iter().map(|(t, p)| (WINDOW, *t, *p)).collect();
        pane_list_of(&tabs)
    }

    /// A pane list of the tabs given, each in the remote window named first.
    fn pane_list_of(tabs: &[(WindowId, TabId, &[PaneId])]) -> ListPanesResponse {
        let mut list = ListPanesResponse {
            tabs: vec![],
            tab_titles: vec![],
            window_titles: HashMap::new(),
        };
        for &(w, t, pane_ids) in tabs {
            let node = match pane_ids {
                [one] => leaf(w, t, *one, size(80)),
                [first, second] => PaneNode::Split {
                    left: Box::new(leaf(w, t, *first, size(40))),
                    right: Box::new(leaf(w, t, *second, size(39))),
                    node: SplitDirectionAndSize {
                        direction: SplitDirection::Horizontal,
                        first: size(40),
                        second: size(39),
                    },
                },
                _ => unimplemented!("one or two panes per tab"),
            };
            list.tabs.push(node);
            list.tab_titles.push(format!("tab {t}"));
        }
        list
    }

    /// `wezterm cli split-pane --move-pane-id` moves the only pane of one
    /// tab into another, and the server removes the emptied tab. The resync
    /// that follows must drop the local copy of that tab, which otherwise
    /// keeps showing the moved pane, and must not ask the server to kill the
    /// pane, which lives on in the other tab.
    #[test]
    fn a_tab_emptied_by_a_move_is_dropped_and_its_pane_kept() {
        let f = Fixture::new();
        f.resync(pane_list(&[(1, &[10]), (2, &[20])]));
        let (p10, p20) = (f.local_pane(10), f.local_pane(20));
        let t2 = f.local_tab(2);
        assert_eq!(f.window_tabs(), vec![vec![p10], vec![p20]]);

        f.resync(pane_list(&[(1, &[10, 20])]));

        assert_eq!(f.window_tabs(), vec![vec![p10, p20]]);
        assert!(f.mux.get_tab(t2).is_none());
        assert!(f.mux.get_pane(p20).is_some());
        assert_eq!(f.kills_sent(), Vec::<PaneId>::new());
    }

    /// A tab whose panes the server no longer lists at all is dropped with
    /// them, and no kill is sent for panes that are already gone there.
    #[test]
    fn a_tab_the_server_closed_is_dropped_with_its_panes() {
        let f = Fixture::new();
        f.resync(pane_list(&[(1, &[10]), (2, &[20, 21])]));
        let p10 = f.local_pane(10);
        let (p20, p21) = (f.local_pane(20), f.local_pane(21));

        f.resync(pane_list(&[(1, &[10])]));

        assert_eq!(f.window_tabs(), vec![vec![p10]]);
        assert!(f.mux.get_pane(p20).is_none());
        assert!(f.mux.get_pane(p21).is_none());
        assert_eq!(f.kills_sent(), Vec::<PaneId>::new());
    }

    /// A list requested before the client learned of a tab (a spawn whose
    /// reply arrived before the list) does not show that tab, and must not
    /// remove it.
    #[test]
    fn a_tab_learned_after_the_request_is_kept() {
        let f = Fixture::new();
        f.resync(pane_list(&[(1, &[10])]));
        let known_before_request = f.inner.known_remote_ids();
        f.resync(pane_list(&[(1, &[10]), (2, &[20])]));
        let (p10, p20) = (f.local_pane(10), f.local_pane(20));
        let t2 = f.local_tab(2);

        f.resync_knowing(pane_list(&[(1, &[10])]), known_before_request);

        assert_eq!(f.window_tabs(), vec![vec![p10], vec![p20]]);
        assert_eq!(f.inner.remote_to_local_tab_id(2), Some(t2));
        assert_eq!(f.kills_sent(), Vec::<PaneId>::new());
    }

    /// A tab moved here is sent to the server as the window's new order, in
    /// the server's ids, to each remote window mapped to the local window:
    /// two spawns into one window that has no remote window yet each get a
    /// remote window of their own.
    #[test]
    fn a_local_move_makes_a_tab_order_request_for_each_remote_window() {
        let f = Fixture::new();
        f.resync(pane_list(&[(1, &[10]), (2, &[20])]));
        let local = f.local_window();
        f.inner
            .record_remote_to_local_window_mapping(OTHER_WINDOW, local);
        f.resync(pane_list_of(&[
            (WINDOW, 1, &[10]),
            (WINDOW, 2, &[20]),
            (OTHER_WINDOW, 3, &[30]),
        ]));
        let (t1, t2, t3) = (f.local_tab(1), f.local_tab(2), f.local_tab(3));
        f.move_tabs_locally(&[t3, t1, t2]);

        let remote_windows = f.inner.remote_windows_of(local);
        let mut requests = f.inner.tab_order_requests(local, &remote_windows);
        requests.sort_by_key(|request| request.window_id);
        assert_eq!(
            requests,
            vec![
                SetWindowTabOrder {
                    window_id: WINDOW,
                    tab_ids: vec![3, 1, 2],
                },
                SetWindowTabOrder {
                    window_id: OTHER_WINDOW,
                    tab_ids: vec![3, 1, 2],
                },
            ]
        );
    }

    /// The client asks once whether the server takes tab orders, with an
    /// empty order: a server that answers takes them, one that calls the PDU
    /// invalid predates them, and any other failure is asked again later.
    #[test]
    fn the_server_is_asked_whether_it_takes_tab_orders() {
        let cases: [(&str, Option<&str>, TabOrderSupport); 3] = [
            ("answered", None, TabOrderSupport::Yes),
            (
                "older server",
                Some("Error: invalid PDU Invalid { ident: 63 }"),
                TabOrderSupport::No,
            ),
            (
                "window gone",
                Some("Error: window 100 is invalid"),
                TabOrderSupport::Unknown,
            ),
        ];
        for (case, refusal, support) in cases {
            let f = Fixture::new();
            f.resync(pane_list(&[(1, &[10])]));
            let inner = Arc::clone(&f.inner);
            let asked =
                f.tab_orders_answered(
                    refusal,
                    async move { inner.learn_tab_order_support().await },
                );
            assert_eq!(
                asked,
                vec![SetWindowTabOrder {
                    window_id: WINDOW,
                    tab_ids: vec![],
                }],
                "{case}"
            );
            assert_eq!(f.inner.tab_order_support(), support, "{case}");
        }

        let f = Fixture::new();
        f.resync(pane_list(&[(1, &[10])]));
        f.inner.set_tab_order_support(TabOrderSupport::Yes);
        let inner = Arc::clone(&f.inner);
        let asked =
            f.tab_orders_answered(None, async move { inner.learn_tab_order_support().await });
        assert_eq!(asked, vec![]);
    }

    /// A move that the server refuses as an invalid PDU shows that it
    /// predates tab orders, so the next move is not sent.
    #[test]
    fn a_move_refused_as_an_invalid_pdu_stops_further_moves() {
        let f = Fixture::new();
        f.resync(pane_list(&[(1, &[10]), (2, &[20])]));
        let local = f.local_window();
        let move_once = || {
            let inner = Arc::clone(&f.inner);
            async move { inner.send_tab_orders(local, vec![WINDOW]).await }
        };
        let refusal = Some("Error: invalid PDU Invalid { ident: 63 }");

        assert_eq!(f.tab_orders_answered(refusal, move_once()).len(), 1);
        assert_eq!(f.inner.tab_order_support(), TabOrderSupport::No);
        assert_eq!(f.tab_orders_answered(refusal, move_once()), vec![]);
    }

    /// A server that takes tab orders owns them: a tab another client added
    /// and moved before this client's next pane list is shown in the
    /// server's place, not after the tabs this client already had.
    #[test]
    fn a_tab_added_and_moved_elsewhere_takes_the_servers_place() {
        let f = Fixture::new();
        f.inner.set_tab_order_support(TabOrderSupport::Yes);
        f.resync(pane_list(&[(1, &[10]), (2, &[20])]));
        let (t1, t2) = (f.local_tab(1), f.local_tab(2));

        f.resync(pane_list(&[(3, &[30]), (1, &[10]), (2, &[20])]));

        assert_eq!(f.window_tab_ids(), vec![f.local_tab(3), t1, t2]);
    }

    /// A move made here that another client undid before this client's next
    /// pane list is undone here too: the server's order is shown, whatever
    /// the list before it said.
    #[test]
    fn a_move_undone_on_the_server_is_undone_here() {
        let f = Fixture::new();
        f.inner.set_tab_order_support(TabOrderSupport::Yes);
        f.resync(pane_list(&[(1, &[10]), (2, &[20])]));
        let (t1, t2) = (f.local_tab(1), f.local_tab(2));
        f.move_tabs_locally(&[t2, t1]);

        f.resync(pane_list(&[(1, &[10]), (2, &[20])]));

        assert_eq!(f.window_tab_ids(), vec![t1, t2]);
    }

    /// A pane list asked for before this client sent a move predates the
    /// move, however late it is applied, so it leaves the tabs where they
    /// are; a list asked for after the move shows the server's order.
    #[test]
    fn a_list_requested_before_a_move_keeps_the_move() {
        let f = Fixture::new();
        f.inner.set_tab_order_support(TabOrderSupport::Yes);
        f.resync(pane_list(&[(1, &[10]), (2, &[20]), (3, &[30])]));
        let (t1, t2, t3) = (f.local_tab(1), f.local_tab(2), f.local_tab(3));
        let known_before_the_move = f.inner.known_remote_ids();
        f.move_tabs_locally(&[t2, t3, t1]);
        f.inner.count_tab_move();

        f.resync_knowing(
            pane_list(&[(2, &[20]), (1, &[10]), (3, &[30])]),
            known_before_the_move,
        );
        assert_eq!(f.window_tab_ids(), vec![t2, t3, t1]);

        f.resync(pane_list(&[(3, &[30]), (1, &[10]), (2, &[20])]));
        assert_eq!(f.window_tab_ids(), vec![t3, t1, t2]);
    }

    /// A server that does not take tab orders keeps its own order. A resync
    /// must not undo the move made in this client, also when the server
    /// added a tab since.
    #[test]
    fn a_local_move_survives_a_server_that_does_not_take_orders() {
        let f = Fixture::new();
        f.inner.set_tab_order_support(TabOrderSupport::No);
        f.resync(pane_list(&[(1, &[10]), (2, &[20]), (3, &[30])]));
        let (t1, t2, t3) = (f.local_tab(1), f.local_tab(2), f.local_tab(3));
        f.move_tabs_locally(&[t3, t1, t2]);

        f.resync(pane_list(&[(1, &[10]), (2, &[20]), (3, &[30]), (4, &[40])]));

        assert_eq!(f.window_tab_ids(), vec![t3, t1, t2, f.local_tab(4)]);
    }
}
