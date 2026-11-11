use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, TitleBar, WindowExt as _,
    button::ButtonVariant,
    dialog::DialogButtonProps,
    dock::{DockArea, DockEvent, DockLayout, DockPlacement, PanelId, TabGroup, panel_handle},
    form::{Field, Form},
    h_flex,
    input::{Input, InputContentType, InputState},
    notification::Notification,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::front::follow_focus;
use crate::analytics::{Analytics, AnalyticsServices, Counter};
use crate::app::{
    CenterTab, ClearTerminal, CloseActiveTab, CloseLocalTerminal, CloseSettings, CloseTabs,
    CloseTerminal, CollapseAllGroups, ConnectGroup, ConnectHost, CopyHostAddress, CopyHostId,
    CopyTerminal, DeleteGroup, DeleteHost, DisconnectTerminal, DismissTerminalFind, DuplicateHost,
    DuplicateTerminal, EditHost, ExpandAllGroups, FindInTerminal, FindNextInTerminal,
    FindPreviousInTerminal, FocusSearch, MaximizeWindow, MoveHostNode, NewChildGroup, NewGroup,
    NewHost, NewHostInGroup, NewLocalTerminal, NewTemporaryConnection, OpenSettings, PasteTerminal,
    QuickConnect, ReconnectTerminal, RenameGroup, RenameTerminal, RestartLocalTerminal,
    RestoreWindow, SelectAllTerminal, SendBreak, ShowOrHideApp, ToggleHostPanel, ToggleTheme,
    ToolKind, ZoomIn, ZoomOut, ZoomReset, ZoomTerminalIn, ZoomTerminalOut, ZoomTerminalReset,
};
use crate::connection::{
    ConnectionPrompt, ConnectionPromptField, ConnectionPromptKind, ConnectionPromptReply,
    ConnectionSecret, SharedConnectionTester,
};
use crate::credential::CredentialPanel;
use crate::forward::{
    ForwardManager, ForwardManagerEvent, ForwardPanel, SharedForwardTransportProvider,
    SshForwardTransportProvider,
};
use crate::host::{
    Dependents, ForwardId, GroupId, HostId, HostNode, HostPanel, HostStore, HostStoreEvent,
    SavedTerminal, SavedTerminalId, confirm_delete_group, confirm_delete_host, open_group_dialog,
    open_host_dialog, open_quick_connect_dialog, open_temporary_connection_dialog,
};
use crate::i18n::t;
use crate::instance::InstanceServer;
use crate::settings::{
    Appearance, DEFAULT_INTERFACE_FONT_SIZE, SettingsPanel, SettingsPanelEvent, SettingsStore,
    SettingsStoreEvent, WindowSettings,
};
use crate::shared::{commit_footer, open_rename_tab_dialog};
use crate::ssh::{
    authentication_fields, authentication_instructions, authentication_title,
    unknown_host_description,
};
use crate::sync::{SyncEvent, SyncService};
use crate::terminal::{
    DEFAULT_FONT_SIZE, HostTerminalTransports, LocalPtyTransportProvider, LocalTerminalId,
    LocalTerminalPanel, LocalTerminalPanelEvent, RemoteTerminalId, SavedName, SearchDirection,
    SharedLocalTerminalTransportProvider, SharedRemoteTerminalTransportProvider,
    SharedSerialTerminalTransportProvider, SharedTelnetTerminalTransportProvider,
    SharedTerminalTransportFactory, TerminalCopy, TerminalPanel, TerminalPanelEvent, TerminalView,
};
use crate::update::{UpdateServices, Updater, UpdaterEvent};

use super::{
    compose::COMPOSE_HEIGHT,
    compose_pane::ComposePane,
    dock_skin::WorkspaceDockSkin,
    recent_hosts::RecentHosts,
    sidebar::Sidebar,
    sidebar_widths::opening_widths,
    status_bar::{WorkspaceStatus, WorkspaceStatusBar},
    title_bar::{SwitchEnd, render_sidebar_toggle, render_title_bar},
    tool_sidebar::{ToolSidebar, render_tool_switch},
    tools::set_right_dock_open,
    window_state::{WindowState, window_placement},
};

const DOCK_ID: &str = "shellrs-dock";
const DOCK_VERSION: usize = 1;
/// How far the zoom keys move 界面字号 a press, in pixels.
const INTERFACE_ZOOM_STEP: f32 = 2.;
/// Show a notification from code that runs while the window is still being
/// built: loading what is on disk, starting the services.
///
/// Notifications belong to the window's `Root`, which is only in place once
/// the closure that builds the window has returned; pushing one before that
/// panics. Deferred, it arrives as soon as the window is there.
pub fn notify_once_open(notification: Notification, window: &mut Window, cx: &mut App) {
    window.defer(cx, move |window, cx| {
        window.push_notification(notification, cx)
    });
}

/// Window options for the main workspace window: where it was last, as much
/// of it as 设置 › 应用 › 窗口 remembers.
pub fn window_options(
    saved: Option<&WindowState>,
    settings: WindowSettings,
    cx: &mut App,
) -> WindowOptions {
    let (window_bounds, display_id) = window_placement(saved, settings, cx);
    WindowOptions {
        window_bounds: Some(window_bounds),
        display_id,
        // Window geometry is a platform boundary; `px` is the API's unit.
        window_min_size: Some(gpui_kit::Size {
            width: px(960.),
            height: px(600.),
        }),
        app_id: Some(crate::app::DESKTOP_APP_ID.into()),
        ..TitleBar::window_options()
    }
}

/// Who a connection question belongs to, and so who gets the answer. The
/// number tells one connection attempt of the owner from the next: an answer
/// to a question of an attempt that is over is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PromptOwner {
    Terminal(RemoteTerminalId),
    Forward(ForwardId, u64),
}

/// The main window content: title bar above the dock, status bar below.
///
/// Owns the host store, the dock and the registry of open per-host
/// panels, and handles every application action.
pub struct Workspace {
    pub(super) broadcast: super::terminal_groups::TerminalBroadcast,
    /// 撰写窗格, in the bottom dock while it shows.
    pub(super) compose: Entity<ComposePane>,
    /// How tall 撰写窗格 was last: it opens again so.
    pub(super) compose_height: Pixels,
    /// Where the focus was when the window stopped being the active one.
    focus_when_left: Option<Option<FocusHandle>>,
    pub(super) store: Entity<HostStore>,
    pub(super) dock_area: Entity<DockArea>,
    pub(super) skin: Rc<WorkspaceDockSkin>,
    /// The left dock's panel: the host tree or the forward list.
    pub(super) sidebar: Entity<Sidebar>,
    /// The right dock's panel: the tools for the host of the tab in front.
    pub(super) tools: Entity<ToolSidebar>,
    pub(super) host_panel: Entity<HostPanel>,
    /// The port forwards that are running, each on a connection of its own.
    pub(super) forwards: Entity<ForwardManager>,
    /// The start page the dock skin shows while the center has no tab.
    recent: Entity<RecentHosts>,
    pub(super) terminals: HashMap<RemoteTerminalId, Entity<TerminalPanel>>,
    pub(super) local_terminals: HashMap<LocalTerminalId, Entity<LocalTerminalPanel>>,
    /// The local terminals kept in 项目, by the tab each is open in.
    pub(super) saved_terminals: HashMap<LocalTerminalId, SavedTerminalId>,
    /// The saved terminal's tab in front last, for the next launch to put
    /// in front again.
    pub(super) front_saved_terminal: Option<LocalTerminalId>,
    /// The settings tab, while it is open. There is only ever one.
    settings_tab: Option<Entity<SettingsPanel>>,
    /// What the settings tab edits; the workspace applies it to the window.
    settings: Entity<SettingsStore>,
    /// Hears ShellRS being opened again. Production only: UI tests never
    /// listen on the real socket.
    pub(super) instance_server: Option<InstanceServer>,
    pub(super) locked_links: Vec<crate::instance::OpenLink>,
    /// Looks for, downloads and installs newer versions. Without services
    /// in UI tests, so it never reaches the network unless a test hands it
    /// fakes.
    pub(super) updater: Entity<Updater>,
    /// 同步. Without services in UI tests, so it never reaches a folder
    /// unless a test hands it a store.
    pub(super) sync: Entity<SyncService>,
    /// 匿名使用统计. Production only, like the CLI server: UI tests hand in
    /// one that only records.
    pub(super) analytics: Option<super::analytics::Reporting>,
    local_terminal_provider: SharedLocalTerminalTransportProvider,
    host_terminals: HostTerminalTransports,
    /// Backs the host dialog's 「测试连接」.
    connection_tester: SharedConnectionTester,
    next_remote_terminal_id: u64,
    next_local_terminal_id: u64,
    /// The center tab in front: shown most recently, or, with the center
    /// split, the one the focus went into last (see `front.rs`).
    /// `CloseActiveTab` closes it. Changed through `set_active_tab`, which
    /// brings the right sidebar in line.
    pub(super) active_tab: Option<CenterTab>,
    /// The terminal the right sidebar goes with while it shows: the last
    /// one in front, SSH or local. Another tab in front of the other half
    /// of a split leaves it be.
    pub(super) tool_target_choice: Option<super::tools::ToolTarget>,
    /// The right sidebar's tool for the terminals of a host deleted while
    /// they are open: the store no longer has the host to keep it on.
    pub(super) deleted_host_tools: HashMap<HostId, ToolKind>,
    pub(super) prompt_queue: VecDeque<(PromptOwner, HostId, ConnectionPrompt)>,
    pub(super) active_prompt: Option<(PromptOwner, HostId, u64)>,
    /// Dispatch target for the title bar and start page: actions sent to it
    /// reach the workspace handlers whatever is focused.
    pub(super) focus_handle: FocusHandle,
    /// The file transfers that are running, each on a terminal's own
    /// connection.
    pub(super) transfers: Entity<crate::transfer::TransferManager>,
    /// When each terminal last notified, of each kind, keyed by
    /// `notices::notice_tag` and the kind.
    pub(super) notice_times: HashMap<String, std::time::Instant>,
    /// The pending write of the window's size and place, see
    /// `window_state.rs`.
    pub(super) window_state_save: Option<Task<()>>,
    /// The sidebars' widths as the settings last had them from this
    /// workspace, and the pending write of new ones; see
    /// `sidebar_widths.rs`.
    pub(super) sidebar_widths: crate::settings::SidebarWidths,
    pub(super) sidebar_widths_save: Option<Task<()>>,
    /// Where the title bar's list switch ends, for the sidebar toggle.
    pub(super) switch_end: SwitchEnd,
    /// Where 导入 reads `~/.ssh/config` and `known_hosts`. Production only:
    /// UI tests hand in a temporary one, so nothing reads the home
    /// directory.
    pub(super) ssh_dir: Option<PathBuf>,
    pub(super) _subscriptions: Vec<Subscription>,
}

impl Workspace {
    /// `store` is built by `main` from the database on disk, and by the UI
    /// tests from `HostStore::seed`; `settings` likewise from the settings
    /// file, or kept in memory.
    pub fn new(
        store: Entity<HostStore>,
        settings: Entity<SettingsStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_with_instance_server(store, settings, None, window, cx)
    }

    /// Take over the activation listener reserved by the startup password gate.
    pub fn new_with_instance_server(
        store: Entity<HostStore>,
        settings: Entity<SettingsStore>,
        server: Option<InstanceServer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let connector = crate::ssh::SshConnector::new(
            crate::app::data_dir().join("known_hosts"),
            crate::secrets::KeychainSource::shared(store.read(cx).secrets()),
        );
        let remote = Arc::new(crate::ssh::SshTerminalTransportProvider::with_connector(
            connector.clone(),
        ));
        let tester = Arc::new(crate::ssh::SshConnectionTester::new(connector.clone()));
        let forward = Arc::new(SshForwardTransportProvider::new(connector.clone()));
        let mut this = Self::new_with_services(
            store,
            settings,
            remote,
            Arc::new(crate::telnet::TelnetTransportProvider::new(
                connector.clone(),
            )),
            Arc::new(crate::serial::SerialTransportProvider),
            Arc::new(LocalPtyTransportProvider),
            tester,
            forward,
            window,
            cx,
        );
        this.ssh_dir = dirs::home_dir().map(|home| home.join(".ssh"));
        this.instance_server = server;
        this.serve_reopening(window, cx);
        this.remember_window(crate::app::window_state_path(), window, cx);
        this.compose.update(cx, |pane, _| {
            pane.keep_history(crate::app::compose_history_path())
        });
        let bundle = cx.app_path().ok();
        this.updater.update(cx, |updater, cx| {
            updater.set_services(UpdateServices::system(bundle), cx);
            updater.start(cx);
        });
        this.sync.update(cx, |sync, cx| {
            sync.set_services(crate::sync::system_services(), cx)
        });
        if let Some(services) = AnalyticsServices::system() {
            let enabled = this.settings.read(cx).settings().analytics.enabled;
            this.set_analytics(Analytics::start(services, enabled), window, cx);
            this.report_started(cx);
            this.count_automatic_forwards(cx);
        }
        this
    }

    /// Inject every filesystem and transport service; tests require neither a server nor a keychain.
    // One parameter per injected service, which is the point of this
    // constructor; bundling them would only move the list elsewhere.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_services(
        store: Entity<HostStore>,
        settings: Entity<SettingsStore>,
        remote_terminal_provider: SharedRemoteTerminalTransportProvider,
        telnet_terminal_provider: SharedTelnetTerminalTransportProvider,
        serial_terminal_provider: SharedSerialTerminalTransportProvider,
        local_terminal_provider: SharedLocalTerminalTransportProvider,
        connection_tester: SharedConnectionTester,
        forward_provider: SharedForwardTransportProvider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let host_terminals = HostTerminalTransports {
            ssh: remote_terminal_provider,
            telnet: telnet_terminal_provider,
            serial: serial_terminal_provider,
        };
        let recent = cx.new(|cx| RecentHosts::new(store.clone(), focus_handle.clone(), cx));
        let compose =
            cx.new(|cx| ComposePane::new(store.clone(), focus_handle.clone(), window, cx));
        let (dock_area, skin) = WorkspaceDockSkin::dock_area(
            DOCK_ID,
            Some(DOCK_VERSION),
            recent.clone().into(),
            focus_handle.clone(),
            window,
            cx,
        );
        let host_panel = cx.new(|cx| HostPanel::new(store.clone(), window, cx));
        let forwards = cx.new(|cx| ForwardManager::new(store.clone(), forward_provider, cx));
        let transfers = cx.new(|_| crate::transfer::TransferManager::default());
        let update_settings = settings.read(cx).settings().update.clone();
        let updater = cx.new(|cx| {
            let mut updater = Updater::new();
            updater.set_automatic(update_settings.automatic, cx);
            updater.set_channel(update_settings.channel, cx);
            updater
        });
        let sync_location = settings.read(cx).settings().sync.location.clone();
        let sync = cx.new(|cx| {
            let mut sync = SyncService::new(store.clone(), cx);
            sync.set_location(sync_location, cx);
            sync
        });
        let forward_panel = cx.new(|cx| {
            ForwardPanel::new(
                store.clone(),
                forwards.clone(),
                focus_handle.clone(),
                window,
                cx,
            )
        });
        let credential_panel =
            cx.new(|cx| CredentialPanel::new(store.clone(), focus_handle.clone(), window, cx));
        let sidebar = cx.new(|_| Sidebar::new(host_panel.clone(), forward_panel, credential_panel));
        let tools = cx.new(|cx| ToolSidebar::new(store.clone(), focus_handle.clone(), window, cx));
        // Start with focus in the host panel so window-level actions have a
        // dispatch path. The workspace's own handle is never focused: the
        // dialog layer is its child, and a focused ancestor would keep the
        // dialog's focus trap from taking focus.
        let panel_focus = host_panel.read(cx).focus_handle(cx);
        window.focus(&panel_focus, cx);
        // Before the first frame, so a dark theme never starts out light.
        crate::settings::apply(settings.read(cx).settings(), window, cx);

        let mut sidebar_width = dock_area.read(cx).dock_size(DockPlacement::Left);
        let mut subscriptions = vec![
            // What the saved terminals show, for the next launch to draw: the
            // workspace goes with its window, as ShellRS quits (and on
            // Windows and Linux as the window closes, before it quits).
            // So are the sidebars' widths, if a drag has not settled.
            cx.on_release(|this, cx| {
                this.keep_on_close(cx);
                this.save_sidebar_widths(cx);
            }),
            // Neither sidebar is dragged narrower than its minimum, nor
            // 撰写窗格 lower than a line or two.
            cx.observe_in(&dock_area, window, move |this, area, window, cx| {
                this.hold_sidebar_widths(window, cx);
                this.hold_compose_height(window, cx);
                this.remember_sidebar_widths(cx);
                // Keep every entry point, including search and the list
                // switches that reopen the sidebar, for the next launch.
                let collapsed = !area.read(cx).is_dock_open(DockPlacement::Left);
                if this.settings.read(cx).settings().sidebar_collapsed != collapsed {
                    this.settings.update(cx, |store, cx| {
                        store.update(|settings| settings.sidebar_collapsed = collapsed, cx);
                    });
                }
                // The title-bar toggle follows the left dock's current width.
                let width = area.read(cx).dock_size(DockPlacement::Left);
                if sidebar_width != width {
                    sidebar_width = width;
                    cx.notify();
                }
            }),
            cx.observe(&transfers, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &transfers,
                window,
                |this, _, event: &crate::transfer::TransferEvent, window, cx| {
                    this.on_transfer_event(event, window, cx)
                },
            ),
            cx.observe_in(&store, window, |this, _, window, cx| {
                // The system a host runs decides which tools it is offered.
                this.sync_tool_sidebar(window, cx);
                cx.notify()
            }),
            cx.subscribe_in(
                &store,
                window,
                |_, _, event: &HostStoreEvent, window, cx| match event {
                    HostStoreEvent::PersistFailed(message) => {
                        window.push_notification(Notification::error(message.clone()), cx);
                    }
                    // The forward manager restarts the rule itself.
                    HostStoreEvent::ForwardSettingsChanged(_) => {}
                },
            ),
            cx.observe(&store, |this, _, cx| this.on_saved_terminals_changed(cx)),
            cx.subscribe_in(
                &forwards,
                window,
                |this, _, event: &ForwardManagerEvent, window, cx| {
                    this.on_forward_event(event, window, cx)
                },
            ),
            cx.observe_in(&settings, window, |this, settings, window, cx| {
                crate::settings::apply(settings.read(cx).settings(), window, cx);
                if !settings.read(cx).settings().local_terminal.keep_output {
                    this.store.read(cx).forget_kept_outputs();
                }
                this.sync_updater(cx);
                this.follow_sync_settings(cx);
                this.sync_analytics(window, cx);
                if !settings
                    .read(cx)
                    .settings()
                    .sidebar_tools
                    .shows(crate::app::ToolKind::Kubernetes)
                {
                    this.tools
                        .update(cx, |tools, cx| tools.disable_kubernetes(cx));
                }
                this.sync_tool_sidebar(window, cx);
                this.follow_sidebar_widths(window, cx);
            }),
            cx.subscribe_in(
                &updater,
                window,
                |this, _, event: &UpdaterEvent, window, cx| {
                    this.on_updater_event(event, window, cx)
                },
            ),
            cx.subscribe_in(&sync, window, |this, _, event: &SyncEvent, window, cx| {
                this.on_sync_event(event, window, cx)
            }),
            // Coming back to ShellRS, the other computers' edits come too.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.sync.update(cx, |sync, cx| sync.window_activated(cx));
                    this.focus_terminal_on_return(window, cx);
                } else {
                    this.focus_when_left = Some(window.focused(cx));
                }
            }),
            cx.subscribe_in(
                &settings,
                window,
                |_, _, event: &SettingsStoreEvent, window, cx| match event {
                    SettingsStoreEvent::PersistFailed(message) => {
                        window.push_notification(Notification::error(message.clone()), cx);
                    }
                },
            ),
            // 跟随系统 follows the system appearance as it changes.
            cx.observe_window_appearance(window, |this, window, cx| {
                crate::settings::apply(this.settings.read(cx).settings(), window, cx);
            }),
            cx.subscribe_in(
                &dock_area,
                window,
                |this, _, event: &DockEvent, window, cx| {
                    if matches!(event, DockEvent::LayoutChanged) {
                        this.on_layout_changed(window, cx);
                    }
                },
            ),
            // Zoom and sidebar toggles only notify. Keep the permanent
            // broadcast control on the leading visible tab bar.
            cx.observe(&dock_area, |this, _, cx| this.sync_broadcast(cx)),
        ];

        // A fixture's hosts start with a terminal tab each.
        let connected: Vec<HostId> = store.read(cx).starting_terminals().to_vec();
        // A click on a terminal's system notification shows that terminal.
        let this = cx.weak_entity();
        let handle = window.window_handle();
        cx.on_system_notification_response(move |response, cx| {
            _ = handle.update(cx, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    this.on_notice_clicked(&response.tag, window, cx)
                });
            });
        });
        // ⌘Q (and on Windows and Linux the window's close button) waits for
        // a last round of sync.
        let this = cx.weak_entity();
        crate::app::set_quit_guard(
            window.window_handle(),
            move |_, cx| {
                this.update(cx, |this, cx| this.sync_before_quit(cx))
                    .unwrap_or(false)
            },
            cx,
        );
        let mut terminals = HashMap::new();
        let mut center = DockLayout::tabs();
        let mut next_remote_terminal_id = 1;
        let (restore, restore_front) = super::projects::terminals_to_restore(store.read(cx));
        for id in connected {
            let terminal_id = RemoteTerminalId(next_remote_terminal_id);
            next_remote_terminal_id += 1;
            let (panel, subscription) = new_terminal_panel(
                &store,
                host_terminals.clone(),
                terminal_id,
                id,
                None,
                window,
                cx,
            );
            center = center.panel_view(panel_handle(panel.clone()), cx);
            subscriptions.push(subscription);
            let focus = panel.read(cx).focus_handle(cx);
            subscriptions.push(follow_focus(
                CenterTab::Terminal(terminal_id),
                &focus,
                window,
                cx,
            ));
            terminals.insert(terminal_id, panel);
        }
        // The saved terminals the last launch left open, after a fixture's
        // terminals, with the one in front then in front again.
        let mut local_terminals = HashMap::new();
        let mut saved_terminals = HashMap::new();
        let mut next_local_terminal_id = 1;
        let launch_settings = settings.read(cx).settings();
        for saved in restore {
            let id = LocalTerminalId(next_local_terminal_id);
            next_local_terminal_id += 1;
            let replay = super::projects::kept_replay(store.read(cx), &launch_settings, saved.id);
            let factory = local_terminal_provider.factory_for(&super::projects::local_launch(
                &launch_settings,
                Some(&saved),
                replay,
            ));
            let (panel, subscription) = new_local_terminal_panel(
                id,
                factory,
                Some(super::projects::saved_name(&saved)),
                window,
                cx,
            );
            center = center.panel_view(panel_handle(panel.clone()), cx);
            subscriptions.push(subscription);
            let focus = panel.read(cx).focus_handle(cx);
            subscriptions.push(follow_focus(
                CenterTab::LocalTerminal(id),
                &focus,
                window,
                cx,
            ));
            local_terminals.insert(id, panel);
            saved_terminals.insert(id, saved.id);
        }
        let front_saved_terminal = restore_front.map(|index| LocalTerminalId(index as u64 + 1));
        if let Some(index) = restore_front {
            center = center.active_index(terminals.len() + index);
        }
        let center_empty = terminals.is_empty() && local_terminals.is_empty();
        let (left_width, right_width) = opening_widths(launch_settings.sidebar_widths);

        dock_area.update(cx, |area, cx| {
            // With nothing to show, the center keeps the empty layout the
            // area starts with; the skin draws the start page over it.
            if !center_empty {
                area.set_center(center, window, cx);
            }
            area.set_dock(
                DockPlacement::Left,
                DockLayout::tabs().panel_view(panel_handle(sidebar.clone()), cx),
                window,
                cx,
            );
            // Dock geometry is an API boundary that takes `Pixels`.
            area.set_dock_size(DockPlacement::Left, left_width, window, cx);
            area.set_dock_collapsible(DockPlacement::Left, true, window, cx);
            if launch_settings.sidebar_collapsed {
                area.toggle_dock(DockPlacement::Left, window, cx);
            }
            area.set_dock(
                DockPlacement::Right,
                DockLayout::tabs().panel_view(panel_handle(tools.clone()), cx),
                window,
                cx,
            );
            area.set_dock_size(DockPlacement::Right, right_width, window, cx);
            // Hidden until a tool is picked; see `set_right_dock_open`.
            set_right_dock_open(area, false, window, cx);
            area.set_dock_collapsible(DockPlacement::Right, false, window, cx);
        });
        // Settled before the first `LayoutChanged` arrives, so the handler
        // does not take the initial state for a tab having just closed.
        skin.set_center_empty(center_empty, cx);
        if let Some(lock) = crate::app_lock::AppLock::global(cx) {
            lock.update(cx, |lock, cx| lock.record_activity(cx));
            Self::watch_idle_lock(lock.clone(), window, cx);
            subscriptions.push(cx.observe_in(&lock, window, |this, _, window, cx| {
                if !crate::app_lock::is_locked(cx) {
                    for link in std::mem::take(&mut this.locked_links) {
                        this.open_link(link, window, cx);
                    }
                    // Unlocked with the keyboard nowhere (what had it went
                    // with a dialog the lock closed): back to the center.
                    cx.defer_in(window, |this, window, cx| {
                        if !crate::app_lock::is_locked(cx) && window.focused(cx).is_none() {
                            this.focus_center(window, cx);
                        }
                    });
                }
            }));
        }
        // Rules marked to start with the application. What they need to ask
        // arrives as events, once the workspace is there to show it.
        forwards.update(cx, |forwards, cx| forwards.start_automatic(cx));

        let this = Self {
            store,
            dock_area,
            skin,
            sidebar,
            tools,
            host_panel,
            forwards,
            recent,
            terminals,
            local_terminals,
            saved_terminals,
            front_saved_terminal,
            settings_tab: None,
            settings,
            instance_server: None,
            locked_links: Vec::new(),
            updater,
            sync,
            analytics: None,
            local_terminal_provider,
            host_terminals,
            connection_tester,
            next_remote_terminal_id,
            next_local_terminal_id,
            active_tab: None,
            broadcast: Default::default(),
            compose,
            compose_height: COMPOSE_HEIGHT,
            focus_when_left: None,
            tool_target_choice: None,
            deleted_host_tools: HashMap::new(),
            prompt_queue: VecDeque::new(),
            active_prompt: None,
            focus_handle,
            transfers,
            notice_times: HashMap::new(),
            window_state_save: None,
            sidebar_widths: launch_settings.sidebar_widths,
            sidebar_widths_save: None,
            switch_end: SwitchEnd::default(),
            ssh_dir: None,
            _subscriptions: subscriptions,
        };
        if launch_settings.sidebar_collapsed {
            // The hidden host panel cannot provide a keyboard dispatch path.
            let focus = this
                .visible_center_tabs(cx)
                .first()
                .and_then(|tab| this.tab_focus_handle(*tab, cx))
                .unwrap_or_else(|| this.recent.read(cx).focus_handle(cx));
            window.focus(&focus, cx);
        }
        this
    }

    /// The center tab in front.
    pub fn active_tab(&self) -> Option<CenterTab> {
        self.active_tab
    }

    /// The host store, for tests and for panels created later.
    /// The dock area: the center's tabs and the two sidebars.
    pub fn dock_area(&self) -> &Entity<DockArea> {
        &self.dock_area
    }

    pub fn store(&self) -> &Entity<HostStore> {
        &self.store
    }

    pub fn terminal(&self, id: HostId, cx: &App) -> Option<&Entity<TerminalPanel>> {
        self.terminals
            .values()
            .filter(|panel| panel.read(cx).host_id() == id)
            .max_by_key(|panel| panel.read(cx).id().0)
    }

    pub fn remote_terminal(&self, id: RemoteTerminalId) -> Option<&Entity<TerminalPanel>> {
        self.terminals.get(&id)
    }

    pub fn terminal_count(&self, id: HostId, cx: &App) -> usize {
        self.terminals_of(id, cx).count()
    }

    /// A host's terminal tabs, in no particular order.
    fn terminals_of<'a>(
        &'a self,
        host: HostId,
        cx: &'a App,
    ) -> impl Iterator<Item = &'a Entity<TerminalPanel>> {
        self.terminals
            .values()
            .filter(move |panel| panel.read(cx).host_id() == host)
    }

    /// Whether a host has any tab open in the center.
    pub(super) fn has_tabs(&self, host: HostId, cx: &App) -> bool {
        self.terminals_of(host, cx).next().is_some()
    }

    /// Whether one of the host's terminals is connected. Each tab goes by
    /// its own connection; this is for whoever asks after the host as a
    /// whole.
    pub fn has_connection(&self, host: HostId, cx: &App) -> bool {
        self.terminals_of(host, cx)
            .any(|panel| panel.read(cx).connection_state(cx).is_connected())
    }

    pub fn local_terminal(&self, id: LocalTerminalId) -> Option<&Entity<LocalTerminalPanel>> {
        self.local_terminals.get(&id)
    }

    pub fn settings_tab(&self) -> Option<&Entity<SettingsPanel>> {
        self.settings_tab.as_ref()
    }

    pub fn settings(&self) -> &Entity<SettingsStore> {
        &self.settings
    }

    /// The running port forwards, for tests.
    pub fn forwards(&self) -> &Entity<ForwardManager> {
        &self.forwards
    }

    pub fn updater(&self) -> &Entity<Updater> {
        &self.updater
    }

    pub fn sync(&self) -> &Entity<SyncService> {
        &self.sync
    }

    /// Give the focus to the center: the tab shown last, or the start page
    /// while there is none. For when the focus is about to go off screen.
    pub(super) fn focus_center(&self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self
            .active_tab
            .and_then(|tab| self.tab_focus_handle(tab, cx));
        let handle = tab.or_else(|| {
            self.skin
                .is_center_empty()
                .then(|| self.recent.read(cx).focus_handle(cx))
        });
        if let Some(handle) = handle {
            window.focus(&handle, cx);
        }
    }

    /// Back from another app, typing goes to the terminal shown, wherever
    /// the focus was left. A dialog keeps it, and so does 撰写窗格, which
    /// types for the terminal.
    fn focus_terminal_on_return(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // What brought the window back may have put the focus somewhere
        // itself: a click, a command.
        if self.focus_when_left.take() != Some(window.focused(cx)) {
            return;
        }
        let on_terminal = matches!(
            self.active_tab,
            Some(CenterTab::Terminal(_) | CenterTab::LocalTerminal(_))
        );
        if on_terminal
            && !window.has_active_dialog(cx)
            && !self
                .compose
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
        {
            self.focus_center(window, cx);
        }
    }

    /// Where the focus goes in `tab`.
    pub(super) fn tab_focus_handle(&self, tab: CenterTab, cx: &App) -> Option<FocusHandle> {
        match tab {
            CenterTab::Terminal(id) => self
                .terminals
                .get(&id)
                .map(|panel| panel.read(cx).focus_handle(cx)),
            CenterTab::LocalTerminal(id) => self
                .local_terminals
                .get(&id)
                .map(|panel| panel.read(cx).focus_handle(cx)),
            CenterTab::Settings => self
                .settings_tab
                .as_ref()
                .map(|panel| panel.read(cx).focus_handle(cx)),
        }
    }

    /// Keep the start page and focus in step with the center: once its last
    /// tab closes, the page fills it and takes the focus the tab held, so
    /// window-level shortcuts keep a dispatch path.
    fn on_layout_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let empty = self.dock_area.read(cx).is_empty(DockPlacement::Center, cx);
        let was_empty = self.skin.is_center_empty();
        self.skin.set_center_empty(empty, cx);
        if empty && !was_empty {
            self.recent
                .update(cx, |recent, cx| recent.clear_selection(cx));
            let page = self.recent.read(cx).focus_handle(cx);
            window.focus(&page, cx);
        }
        // A tab dragged into another group may hide the one in front.
        self.sync_front(window, cx);
        self.sync_broadcast(cx);
        self.sync_compose(window, cx);
        // Tabs reordered, moved, opened or closed: what the next launch
        // opens again.
        self.sync_saved_terminals(cx);
    }

    pub(super) fn enqueue_prompt(
        &mut self,
        terminal_id: PromptOwner,
        host_id: HostId,
        prompt: ConnectionPrompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt_queue.push_back((terminal_id, host_id, prompt));
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                let done = this
                    .update_in(cx, |this, window, cx| {
                        if this.prompt_queue.is_empty() {
                            return true;
                        }
                        if crate::app_lock::is_locked(cx)
                            || this.active_prompt.is_some()
                            || window.has_active_dialog(cx)
                        {
                            return false;
                        }
                        this.open_next_prompt(window, cx);
                        true
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    fn open_next_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (terminal_id, host_id, prompt) = loop {
            let Some(next) = self.prompt_queue.pop_front() else {
                return;
            };
            if self.prompt_owner_is_live(next.0, cx) {
                break next;
            }
        };
        let request_id = prompt.request_id();
        self.active_prompt = Some((terminal_id, host_id, request_id));
        let origin = match terminal_id {
            PromptOwner::Forward(id, _) => self.forward_prompt_origin(id, cx),
            PromptOwner::Terminal(_) => None,
        };
        // What a question says first when it needs saying who is asking.
        let introduced = |text: String| match &origin {
            Some(origin) if text.trim().is_empty() => origin.clone(),
            Some(origin) => format!("{origin}\n\n{text}"),
            None => text,
        };
        let workspace = cx.entity().downgrade();
        // Every button of a prompt ends it the same way, with its own reply.
        let answer = move |reply: ConnectionPromptReply, cx: &mut App| {
            workspace
                .update(cx, |this, cx| {
                    this.finish_prompt(terminal_id, request_id, reply, cx)
                })
                .ok();
        };
        match prompt.kind().clone() {
            ConnectionPromptKind::UnknownHost(prompt) => {
                let description = introduced(unknown_host_description(&prompt));
                window.open_alert_dialog(cx, move |alert, _, _| {
                    alert
                        .title(t!("workspace.host_key.unknown_title"))
                        .description(description.clone())
                        .button_props(
                            DialogButtonProps::default()
                                .ok_text(t!("workspace.host_key.trust"))
                                .cancel_text(t!("common.cancel")),
                        )
                        .show_cancel(true)
                        .on_ok({
                            let answer = answer.clone();
                            move |_, _, cx| {
                                answer(ConnectionPromptReply::TrustAndSave, cx);
                                true
                            }
                        })
                        .on_cancel({
                            let answer = answer.clone();
                            move |_, _, cx| {
                                answer(ConnectionPromptReply::Cancel, cx);
                                true
                            }
                        })
                        .on_close({
                            let answer = answer.clone();
                            move |_, _, cx| answer(ConnectionPromptReply::Cancel, cx)
                        })
                });
            }
            ConnectionPromptKind::HostKeyChanged(prompt) => {
                let old = prompt.old_fingerprints().join(&t!("common.list.separator"));
                let description = introduced(
                    match prompt.jump_host() {
                        Some(jump_host) => t!(
                            "workspace.host_key.changed_via_jump_host",
                            jump_host = jump_host,
                            host = prompt.host(),
                            port = prompt.port(),
                            algorithm = prompt.algorithm(),
                            old = old,
                            fingerprint = prompt.fingerprint()
                        ),
                        None => t!(
                            "workspace.host_key.changed",
                            host = prompt.host(),
                            port = prompt.port(),
                            algorithm = prompt.algorithm(),
                            old = old,
                            fingerprint = prompt.fingerprint()
                        ),
                    }
                    .to_string(),
                );
                // Updating is the risky answer: it trusts a key that may be
                // someone posing as the server.
                window.open_alert_dialog(cx, move |alert, _, _| {
                    alert
                        .title(t!("workspace.host_key.changed_title"))
                        .description(description.clone())
                        .button_props(
                            DialogButtonProps::default()
                                .ok_text(t!("workspace.host_key.update"))
                                .ok_variant(ButtonVariant::Danger)
                                .cancel_text(t!("common.cancel")),
                        )
                        .show_cancel(true)
                        .on_ok({
                            let answer = answer.clone();
                            move |_, _, cx| {
                                answer(ConnectionPromptReply::TrustAndSave, cx);
                                true
                            }
                        })
                        .on_cancel({
                            let answer = answer.clone();
                            move |_, _, cx| {
                                answer(ConnectionPromptReply::Cancel, cx);
                                true
                            }
                        })
                        .on_close({
                            let answer = answer.clone();
                            move |_, _, cx| answer(ConnectionPromptReply::Cancel, cx)
                        })
                });
            }
            ConnectionPromptKind::Authentication(prompt) => {
                let title = authentication_title(&prompt);
                let instructions = introduced(authentication_instructions(&prompt));
                let fields = authentication_fields(&prompt);
                let dialog_title: SharedString = if title.trim().is_empty() {
                    t!("workspace.auth.title")
                } else {
                    title.clone().into()
                };
                let form = cx.new(|cx| AuthenticationPromptForm::new(fields, window, cx));
                window.open_dialog(cx, move |dialog, _, _| {
                    dialog
                        .title(dialog_title.clone())
                        .child(
                            v_flex()
                                .gap_3()
                                .when(!instructions.trim().is_empty(), |view| {
                                    view.child(div().text_sm().child(instructions.clone()))
                                })
                                .child(form.clone()),
                        )
                        .footer(commit_footer(
                            "ssh-auth-submit",
                            t!("workspace.auth.continue"),
                        ))
                        .on_ok({
                            let answer = answer.clone();
                            let form = form.clone();
                            move |_, window, cx| {
                                let answers =
                                    form.update(cx, |form, cx| form.take_answers(window, cx));
                                answer(ConnectionPromptReply::Answers(answers), cx);
                                true
                            }
                        })
                        .on_cancel({
                            let answer = answer.clone();
                            move |_, _, cx| {
                                answer(ConnectionPromptReply::Cancel, cx);
                                true
                            }
                        })
                        .on_close({
                            let answer = answer.clone();
                            move |_, _, cx| answer(ConnectionPromptReply::Cancel, cx)
                        })
                        .overlay_closable(false)
                });
            }
        }
    }

    fn finish_prompt(
        &mut self,
        terminal_id: PromptOwner,
        request_id: u64,
        reply: ConnectionPromptReply,
        cx: &mut Context<Self>,
    ) {
        let Some((active_terminal, _, active_request)) = self.active_prompt else {
            return;
        };
        if (active_terminal, active_request) != (terminal_id, request_id) {
            return;
        }
        self.active_prompt = None;
        let canceled = matches!(&reply, ConnectionPromptReply::Cancel);
        self.reply_to_owner(terminal_id, request_id, reply, cx);
        if canceled {
            match terminal_id {
                PromptOwner::Terminal(id) => {
                    if let Some(panel) = self.terminals.get(&id).cloned() {
                        panel.update(cx, |panel, cx| panel.cancel_connection(cx));
                    }
                }
                // Declining the question is declining the forward: it ends
                // as stopped, not as failed.
                PromptOwner::Forward(id, _) => {
                    self.forwards
                        .update(cx, |forwards, cx| forwards.stop(id, cx));
                }
            }
        }
    }

    fn prompt_owner_is_live(&self, owner: PromptOwner, cx: &App) -> bool {
        match owner {
            PromptOwner::Terminal(id) => self.terminals.contains_key(&id),
            PromptOwner::Forward(id, generation) => {
                self.forwards.read(cx).generation(id) == Some(generation)
            }
        }
    }

    fn reply_to_owner(
        &self,
        owner: PromptOwner,
        request_id: u64,
        reply: ConnectionPromptReply,
        cx: &App,
    ) {
        match owner {
            PromptOwner::Terminal(id) => {
                if let Some(panel) = self.terminals.get(&id) {
                    panel.read(cx).reply_to_prompt(request_id, reply, cx);
                }
            }
            PromptOwner::Forward(id, generation) => {
                self.forwards
                    .read(cx)
                    .reply_to_prompt(id, generation, request_id, reply);
            }
        }
    }

    pub(super) fn cancel_prompts_for_owner(
        &mut self,
        owner: PromptOwner,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.prompt_queue.retain(|(id, _, _)| *id != owner);
        if let Some((id, _, request_id)) = self.active_prompt
            && id == owner
        {
            self.active_prompt = None;
            self.reply_to_owner(id, request_id, ConnectionPromptReply::Cancel, cx);
            if window.has_active_dialog(cx) {
                window.close_dialog(cx);
            }
        }
    }
    fn cancel_prompts_for_terminal(
        &mut self,
        terminal_id: RemoteTerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_prompts_for_owner(PromptOwner::Terminal(terminal_id), window, cx);
    }
    pub(super) fn cancel_prompts_for_host(
        &mut self,
        host_id: HostId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A port forward through the host is on its own: what the
        // host's tabs are told does not reach it. Its questions end when
        // the forward does.
        let of_a_tab = |owner: &PromptOwner| !matches!(owner, PromptOwner::Forward(..));
        self.prompt_queue
            .retain(|(owner, id, _)| *id != host_id || !of_a_tab(owner));
        if let Some((owner, id, _)) = self.active_prompt
            && id == host_id
            && of_a_tab(&owner)
        {
            self.cancel_prompts_for_owner(owner, window, cx);
        }
    }

    fn on_connect_host(
        &mut self,
        action: &ConnectHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.connect_host(action.0, window, cx);
    }

    fn on_connect_group(
        &mut self,
        action: &ConnectGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host_ids = self.store.read(cx).hosts_under(action.0);
        for host_id in host_ids {
            self.connect_host(host_id, window, cx);
        }
    }

    pub(super) fn connect_host(
        &mut self,
        host_id: HostId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_remote_terminal(host_id, None, window, cx);
    }

    fn on_duplicate_terminal(
        &mut self,
        action: &DuplicateTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.terminals.get(&action.0) else {
            return;
        };
        let source = source.read(cx);
        let (host_id, copy) = (source.host_id(), source.copy_for_new_tab(cx));
        self.open_remote_terminal(host_id, Some(copy), window, cx);
    }

    fn open_remote_terminal(
        &mut self,
        host_id: HostId,
        copy: Option<TerminalCopy>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.read(cx).host(host_id).is_none() {
            return;
        }
        let terminal_id = RemoteTerminalId(self.next_remote_terminal_id);
        self.next_remote_terminal_id += 1;
        let (panel, subscription) = new_terminal_panel(
            &self.store,
            self.host_terminals.clone(),
            terminal_id,
            host_id,
            copy,
            window,
            cx,
        );
        self._subscriptions.push(subscription);
        let focus = panel.read(cx).focus_handle(cx);
        self._subscriptions.push(follow_focus(
            CenterTab::Terminal(terminal_id),
            &focus,
            window,
            cx,
        ));
        self.terminals.insert(terminal_id, panel.clone());
        self.dock_area.update(cx, |area, cx| {
            area.add_panel_view(panel_handle(panel), DockPlacement::Center, None, window, cx);
        });
    }

    fn on_new_local_terminal(
        &mut self,
        _: &NewLocalTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_local_terminal(None, window, cx);
        self.count(Counter::LocalTerminal);
    }

    /// Open a local terminal tab: in the home folder, or a saved terminal's
    /// in the folder it was last in, named as it is.
    pub(super) fn open_local_terminal(
        &mut self,
        saved: Option<&SavedTerminal>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> LocalTerminalId {
        let id = LocalTerminalId(self.next_local_terminal_id);
        self.next_local_terminal_id += 1;
        let factory = self.local_terminal_factory(saved, true, cx);
        let name = saved.map(super::projects::saved_name);
        let (panel, subscription) = new_local_terminal_panel(id, factory, name, window, cx);
        self._subscriptions.push(subscription);
        let focus = panel.read(cx).focus_handle(cx);
        self._subscriptions.push(follow_focus(
            CenterTab::LocalTerminal(id),
            &focus,
            window,
            cx,
        ));
        self.local_terminals.insert(id, panel.clone());
        if let Some(saved) = saved {
            self.saved_terminals.insert(id, saved.id);
        }
        self.dock_area.update(cx, |area, cx| {
            area.add_panel_view(panel_handle(panel), DockPlacement::Center, None, window, cx);
        });
        id
    }

    /// Open the settings tab, or bring the open one forward: settings are
    /// one place, not one tab per visit.
    fn on_open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.settings_tab.clone() {
            let group = panel.read(cx).tab_group();
            self.activate_tab(group, panel.entity_id(), window, cx);
            // Already displayed, the group reports nothing, so the panel
            // takes the focus itself.
            panel.update(cx, |panel, cx| panel.show(window, cx));
            return;
        }
        let store = self.settings.clone();
        let updater = self.updater.clone();
        let sync = self.sync.clone();
        let panel = cx.new(|cx| SettingsPanel::new(store, updater, sync, window, cx));
        let subscription = cx.subscribe_in(
            &panel,
            window,
            |this, _, event: &SettingsPanelEvent, window, cx| {
                match event {
                    SettingsPanelEvent::Activated => {
                        this.set_active_tab(Some(CenterTab::Settings), window, cx);
                    }
                    SettingsPanelEvent::Closed => {
                        this.settings_tab = None;
                        if this.active_tab == Some(CenterTab::Settings) {
                            this.set_active_tab(None, window, cx);
                        }
                    }
                }
                cx.notify();
            },
        );
        self._subscriptions.push(subscription);
        let focus = panel.read(cx).focus_handle(cx);
        self._subscriptions
            .push(follow_focus(CenterTab::Settings, &focus, window, cx));
        self.settings_tab = Some(panel.clone());
        self.dock_area.update(cx, |area, cx| {
            area.add_panel_view(panel_handle(panel), DockPlacement::Center, None, window, cx);
        });
    }

    fn on_close_settings(
        &mut self,
        _: &CloseSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = self.settings_tab.clone() {
            self.dock_area
                .update(cx, |area, cx| area.remove_panel(panel, window, cx));
        }
    }

    /// Display one tab of a tab group, found by its panel's entity.
    pub(super) fn activate_tab(
        &self,
        group: Option<WeakEntity<TabGroup>>,
        panel: EntityId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = group.and_then(|group| group.upgrade()) else {
            return;
        };
        group.update(cx, |group, cx| {
            let target = PanelId::from(panel);
            if let Some(ix) = group
                .panels()
                .iter()
                .position(|panel| panel.panel_id(cx) == target)
            {
                group.select_tab(ix, window, cx);
            }
        });
    }

    /// Close a host's terminal tab. Goes through the dock area rather
    /// than the tab group: the group refuses to close the last tab of the
    /// center, and here every tab is closable.
    fn on_close_terminal(
        &mut self,
        action: &CloseTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_terminal(action.0, window, cx);
    }

    pub(super) fn close_terminal(
        &mut self,
        id: RemoteTerminalId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = self.terminals.get(&id).cloned() {
            self.dock_area
                .update(cx, |area, cx| area.remove_panel(panel, window, cx));
        }
    }

    pub(super) fn on_close_local_terminal(
        &mut self,
        action: &CloseLocalTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = self.local_terminals.get(&action.0).cloned() {
            self.dock_area
                .update(cx, |area, cx| area.remove_panel(panel, window, cx));
        }
    }

    fn on_close_active_tab(
        &mut self,
        _: &CloseActiveTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab) = self.active_tab {
            self.close_center_tab(tab, window, cx);
        }
    }

    /// Close one center tab through its own close path.
    pub(super) fn close_center_tab(
        &mut self,
        tab: CenterTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match tab {
            CenterTab::Terminal(id) => self.on_close_terminal(&CloseTerminal(id), window, cx),
            CenterTab::LocalTerminal(id) => {
                self.on_close_local_terminal(&CloseLocalTerminal(id), window, cx)
            }
            CenterTab::Settings => self.on_close_settings(&CloseSettings, window, cx),
        }
    }

    /// The tab group holding a center tab, and the tab's panel id in it.
    pub(super) fn center_tab_location(
        &self,
        tab: CenterTab,
        cx: &App,
    ) -> Option<(Entity<TabGroup>, PanelId)> {
        let (group, entity) = match tab {
            CenterTab::Terminal(id) => {
                let panel = self.terminals.get(&id)?;
                (panel.read(cx).tab_group(), panel.entity_id())
            }
            CenterTab::LocalTerminal(id) => {
                let panel = self.local_terminals.get(&id)?;
                (panel.read(cx).tab_group(), panel.entity_id())
            }
            CenterTab::Settings => {
                let panel = self.settings_tab.as_ref()?;
                (panel.read(cx).tab_group(), panel.entity_id())
            }
        };
        Some((group?.upgrade()?, PanelId::from(entity)))
    }

    /// The center tab a dock panel stands for, if it is one of ours.
    pub(super) fn center_tab_for_panel(&self, panel: PanelId) -> Option<CenterTab> {
        let is = |entity: EntityId| PanelId::from(entity) == panel;
        self.terminals
            .iter()
            .find(|(_, entity)| is(entity.entity_id()))
            .map(|(id, _)| CenterTab::Terminal(*id))
            .or_else(|| {
                self.local_terminals
                    .iter()
                    .find(|(_, entity)| is(entity.entity_id()))
                    .map(|(id, _)| CenterTab::LocalTerminal(*id))
            })
            .or_else(|| {
                self.settings_tab
                    .as_ref()
                    .filter(|panel| is(panel.entity_id()))
                    .map(|_| CenterTab::Settings)
            })
    }

    fn on_close_tabs(&mut self, action: &CloseTabs, window: &mut Window, cx: &mut Context<Self>) {
        let Some((group, anchor)) = self.center_tab_location(action.tab, cx) else {
            return;
        };
        let panels: Vec<PanelId> = group
            .read(cx)
            .panels()
            .iter()
            .map(|panel| panel.panel_id(cx))
            .collect();
        let Some(ix) = panels.iter().position(|panel| *panel == anchor) else {
            return;
        };
        let tabs: Vec<CenterTab> = action
            .scope
            .targets(panels.len(), ix)
            .into_iter()
            .filter_map(|target| self.center_tab_for_panel(panels[target]))
            .collect();
        for tab in tabs {
            self.close_center_tab(tab, window, cx);
        }
    }

    fn on_rename_terminal(
        &mut self,
        action: &RenameTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = self.terminals.get(&action.0).cloned() {
            open_rename_tab_dialog(panel, window, cx);
        }
    }

    fn on_copy_host_address(
        &mut self,
        action: &CopyHostAddress,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self
            .store
            .read(cx)
            .host(action.0)
            .map(|host| host.address.clone())
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(host.to_string()));
        window.push_notification(
            Notification::success(t!("workspace.copied", text = host)),
            cx,
        );
    }

    fn on_copy_host_id(
        &mut self,
        action: &CopyHostId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(public_id) = self
            .store
            .read(cx)
            .host(action.0)
            .map(|host| host.public_id.clone())
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(public_id.to_string()));
        window.push_notification(
            Notification::success(t!("workspace.copied_id", id = public_id)),
            cx,
        );
    }

    fn on_restart_local_terminal(
        &mut self,
        action: &RestartLocalTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = self.local_terminals.get(&action.0).cloned() {
            // A saved terminal starts again where its shell was last.
            let saved = self
                .saved_terminals
                .get(&action.0)
                .and_then(|id| self.store.read(cx).saved_terminal(*id))
                .cloned();
            let factory = self.local_terminal_factory(saved.as_ref(), false, cx);
            panel.update(cx, |panel, cx| panel.restart(factory, window, cx));
        }
    }

    /// A transport for a local terminal starting now: the shell
    /// 设置 › 终端 › 本地终端 names at this moment, in the folder a saved
    /// terminal was last in; opening (rather than restarting) it, drawing
    /// what it kept first.
    fn local_terminal_factory(
        &self,
        saved: Option<&SavedTerminal>,
        opening: bool,
        cx: &App,
    ) -> SharedTerminalTransportFactory {
        let settings = self.settings.read(cx).settings();
        let replay = saved.filter(|_| opening).and_then(|saved| {
            super::projects::kept_replay(self.store.read(cx), &settings, saved.id)
        });
        self.local_terminal_provider
            .factory_for(&super::projects::local_launch(&settings, saved, replay))
    }

    /// The terminal of the center tab shown most recently, remote or local.
    fn active_terminal(&self, cx: &App) -> Option<Entity<TerminalView>> {
        match self.active_tab? {
            CenterTab::Terminal(id) => Some(self.terminals.get(&id)?.read(cx).terminal().clone()),
            CenterTab::LocalTerminal(id) => {
                Some(self.local_terminals.get(&id)?.read(cx).terminal().clone())
            }
            CenterTab::Settings => None,
        }
    }

    fn on_copy_terminal(&mut self, _: &CopyTerminal, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(terminal) = self.active_terminal(cx) {
            terminal.update(cx, |terminal, cx| terminal.copy_selection(cx));
        }
    }

    fn on_paste_terminal(&mut self, _: &PasteTerminal, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(terminal) = self.active_terminal(cx) {
            terminal.update(cx, |terminal, cx| terminal.paste_clipboard(cx));
        }
    }

    fn on_select_all_terminal(
        &mut self,
        _: &SelectAllTerminal,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(terminal) = self.active_terminal(cx) {
            terminal.update(cx, |terminal, cx| terminal.select_all(cx));
        }
    }

    fn on_find_in_terminal(
        &mut self,
        _: &FindInTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(terminal) = self.active_terminal(cx) {
            terminal.update(cx, |terminal, cx| terminal.open_find(window, cx));
            self.count(Counter::Find);
        }
    }

    fn on_find_next_in_terminal(
        &mut self,
        _: &FindNextInTerminal,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(terminal) = self.active_terminal(cx) {
            terminal.update(cx, |terminal, cx| {
                terminal.step_find(SearchDirection::Down, cx)
            });
        }
    }

    fn on_find_previous_in_terminal(
        &mut self,
        _: &FindPreviousInTerminal,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(terminal) = self.active_terminal(cx) {
            terminal.update(cx, |terminal, cx| {
                terminal.step_find(SearchDirection::Up, cx)
            });
        }
    }

    fn on_dismiss_terminal_find(
        &mut self,
        _: &DismissTerminalFind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(terminal) = self.active_terminal(cx) {
            terminal.update(cx, |terminal, cx| terminal.dismiss_find(window, cx));
        }
    }

    fn on_clear_terminal(&mut self, _: &ClearTerminal, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(terminal) = self.active_terminal(cx) {
            terminal.update(cx, |terminal, cx| terminal.clear(cx));
        }
    }

    fn on_send_break(&mut self, action: &SendBreak, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(terminal) = self.terminals.get(&action.0) {
            terminal.read(cx).send_break(cx);
        }
    }

    fn on_disconnect_terminal(
        &mut self,
        action: &DisconnectTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = action.0;
        self.cancel_prompts_for_terminal(id, window, cx);
        if let Some(terminal) = self.terminals.get(&id).cloned() {
            terminal.update(cx, |terminal, cx| terminal.disconnect(cx));
        }
    }

    fn on_reconnect_terminal(
        &mut self,
        action: &ReconnectTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = action.0;
        self.cancel_prompts_for_terminal(id, window, cx);
        if let Some(terminal) = self.terminals.get(&id).cloned() {
            terminal.update(cx, |terminal, cx| terminal.reconnect(window, cx));
        }
    }

    fn on_new_host(&mut self, _: &NewHost, window: &mut Window, cx: &mut Context<Self>) {
        open_host_dialog(
            None,
            false,
            None,
            self.store.clone(),
            self.connection_tester.clone(),
            window,
            cx,
        );
    }

    fn on_new_temporary_connection(
        &mut self,
        _: &NewTemporaryConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        open_temporary_connection_dialog(
            self.store.clone(),
            self.connection_tester.clone(),
            self.focus_handle.clone(),
            window,
            cx,
        );
    }

    fn on_quick_connect(&mut self, _: &QuickConnect, window: &mut Window, cx: &mut Context<Self>) {
        open_quick_connect_dialog(self.store.clone(), self.focus_handle.clone(), window, cx);
    }

    fn on_edit_host(&mut self, action: &EditHost, window: &mut Window, cx: &mut Context<Self>) {
        // A host opened from a link is not saved, so there is nothing to
        // edit.
        if self
            .store
            .read(cx)
            .hosts()
            .iter()
            .any(|host| host.id == action.0)
        {
            open_host_dialog(
                Some(action.0),
                self.has_tabs(action.0, cx),
                None,
                self.store.clone(),
                self.connection_tester.clone(),
                window,
                cx,
            );
        }
    }

    fn on_duplicate_host(
        &mut self,
        action: &DuplicateHost,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let copy = self
            .store
            .update(cx, |store, cx| store.duplicate(action.0, cx));
        if let Some(copy) = copy {
            self.host_panel
                .update(cx, |panel, cx| panel.select_node(HostNode::Host(copy), cx));
        }
    }

    fn on_move_host_node(&mut self, action: &MoveHostNode, _: &mut Window, cx: &mut Context<Self>) {
        let moved = self.store.update(cx, |store, cx| {
            store.move_node(action.source, action.destination, cx)
        });
        if moved {
            self.host_panel
                .update(cx, |panel, cx| panel.reveal_node(action.source, cx));
        }
    }

    fn on_delete_host(&mut self, action: &DeleteHost, window: &mut Window, cx: &mut Context<Self>) {
        let id = action.0;
        // Saved hosts only: one opened from a link goes with its tabs.
        let Some(host) = self
            .store
            .read(cx)
            .hosts()
            .iter()
            .find(|host| host.id == id)
            .cloned()
        else {
            return;
        };
        let workspace = cx.entity().downgrade();
        let store = self.store.read(cx);
        let dependents = Dependents {
            forwards: store.forwards_of(id).count(),
            jump_users: store.jump_users(&[id]),
        };
        confirm_delete_host(
            &host,
            dependents,
            Rc::new(move |_, cx| {
                workspace
                    .update(cx, |this, cx| this.remove_host(id, cx))
                    .ok();
            }),
            window,
            cx,
        );
    }

    /// Its open tabs stay: each has the host as it took it.
    fn remove_host(&mut self, id: HostId, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.remove(id, cx);
        });
    }

    fn on_new_host_in_group(
        &mut self,
        action: &NewHostInGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        open_host_dialog(
            None,
            false,
            Some(action.0),
            self.store.clone(),
            self.connection_tester.clone(),
            window,
            cx,
        );
    }

    fn on_new_group(&mut self, _: &NewGroup, window: &mut Window, cx: &mut Context<Self>) {
        open_group_dialog(None, None, self.store.clone(), window, cx);
    }

    fn on_expand_all_groups(
        &mut self,
        _: &ExpandAllGroups,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_all_groups_expanded(true, cx);
    }

    fn on_collapse_all_groups(
        &mut self,
        _: &CollapseAllGroups,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_all_groups_expanded(false, cx);
    }

    fn set_all_groups_expanded(&mut self, expanded: bool, cx: &mut Context<Self>) {
        self.store
            .update(cx, |store, cx| store.set_all_groups_expanded(expanded, cx));
        self.host_panel
            .update(cx, |panel, cx| panel.set_all_groups_expanded(expanded, cx));
    }

    fn on_new_child_group(
        &mut self,
        action: &NewChildGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.read(cx).group(action.0).is_some() {
            open_group_dialog(None, Some(action.0), self.store.clone(), window, cx);
        }
    }

    fn on_rename_group(
        &mut self,
        action: &RenameGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.store.read(cx).group(action.0).is_some() {
            open_group_dialog(Some(action.0), None, self.store.clone(), window, cx);
        }
    }

    fn on_delete_group(
        &mut self,
        action: &DeleteGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = action.0;
        let store = self.store.read(cx);
        let Some(group) = store.group(id) else {
            return;
        };
        let name = group.name.to_string();
        let subgroups = store.descendant_groups(id).len();
        let doomed = store.hosts_under(id);
        let dependents = Dependents {
            forwards: doomed.iter().map(|id| store.forwards_of(*id).count()).sum(),
            jump_users: store.jump_users(&doomed),
        };
        let workspace = cx.entity().downgrade();
        confirm_delete_group(
            &name,
            doomed.len(),
            subgroups,
            dependents,
            Rc::new(move |_, cx| {
                workspace
                    .update(cx, |this, cx| this.remove_group(id, cx))
                    .ok();
            }),
            window,
            cx,
        );
    }

    /// The store cascades the delete; the hosts' open tabs stay.
    fn remove_group(&mut self, id: GroupId, cx: &mut Context<Self>) {
        self.store
            .update(cx, |store, cx| store.remove_group(id, cx));
    }

    fn on_toggle_host_panel(
        &mut self,
        _: &ToggleHostPanel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dock_area.update(cx, |area, cx| {
            area.toggle_dock(DockPlacement::Left, window, cx);
        });
        cx.notify();
    }

    fn on_maximize_window(
        &mut self,
        _: &MaximizeWindow,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        if !window.is_maximized() {
            window.zoom_window();
        }
    }

    fn on_restore_window(&mut self, _: &RestoreWindow, window: &mut Window, _: &mut Context<Self>) {
        if window.is_maximized() {
            crate::app::restore_window(window);
        }
    }

    /// 显示或隐藏 ShellRS: in front of other apps, or away when it already
    /// is.
    pub(super) fn on_show_or_hide_app(
        &mut self,
        _: &ShowOrHideApp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.is_window_active() {
            crate::app::send_back(window, cx);
        } else {
            crate::app::bring_forward(window, cx);
        }
    }

    /// The title bar's quick switch: it picks 应用外观 outright, leaving
    /// 跟随系统 if that was chosen, and the settings observer applies it.
    fn on_toggle_theme(&mut self, _: &ToggleTheme, _: &mut Window, cx: &mut Context<Self>) {
        let appearance = if cx.theme().is_dark() {
            Appearance::Light
        } else {
            Appearance::Dark
        };
        self.settings.update(cx, |settings, cx| {
            settings.update(|settings| settings.appearance = appearance, cx)
        });
    }

    fn on_focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        if !self.dock_area.read(cx).is_dock_open(DockPlacement::Left) {
            self.dock_area.update(cx, |area, cx| {
                area.toggle_dock(DockPlacement::Left, window, cx);
            });
        }
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.focus_search(window, cx));
    }

    /// The interface larger or smaller, through the settings (外观 › 界面字号),
    /// which keep it in range and for the next launch; applying them sizes
    /// the interface.
    fn change_interface_font_size(&mut self, size: impl FnOnce(f32) -> f32, cx: &mut App) {
        self.settings.update(cx, |store, cx| {
            store.update(
                |settings| settings.interface.font_size = size(settings.interface.font_size),
                cx,
            )
        });
    }

    fn on_zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.change_interface_font_size(|size| size + INTERFACE_ZOOM_STEP, cx);
    }

    fn on_zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.change_interface_font_size(|size| size - INTERFACE_ZOOM_STEP, cx);
    }

    fn on_zoom_reset(&mut self, _: &ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.change_interface_font_size(|_| DEFAULT_INTERFACE_FONT_SIZE, cx);
    }

    /// The terminals' text a pixel larger or smaller, through the settings
    /// (终端 › 字号), which keep it in range and for the next launch.
    fn change_terminal_font_size(&mut self, size: impl FnOnce(f32) -> f32, cx: &mut App) {
        self.settings.update(cx, |store, cx| {
            store.update(
                |settings| settings.terminal_font.size = size(settings.terminal_font.size),
                cx,
            )
        });
    }

    fn on_zoom_terminal_in(&mut self, _: &ZoomTerminalIn, _: &mut Window, cx: &mut Context<Self>) {
        self.change_terminal_font_size(|size| size + 1., cx);
    }

    fn on_zoom_terminal_out(
        &mut self,
        _: &ZoomTerminalOut,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_terminal_font_size(|size| size - 1., cx);
    }

    fn on_zoom_terminal_reset(
        &mut self,
        _: &ZoomTerminalReset,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_terminal_font_size(|_| DEFAULT_FONT_SIZE, cx);
    }
}

fn new_terminal_panel(
    store: &Entity<HostStore>,
    transports: HostTerminalTransports,
    terminal_id: RemoteTerminalId,
    host_id: HostId,
    copy: Option<TerminalCopy>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> (Entity<TerminalPanel>, Subscription) {
    let panel = cx.new(|cx| match copy {
        Some(copy) => {
            TerminalPanel::from_copy(terminal_id, copy, store.clone(), transports, window, cx)
        }
        None => TerminalPanel::new(terminal_id, host_id, store.clone(), transports, window, cx),
    });
    let subscription = cx.subscribe_in(
        &panel,
        window,
        |this, _, event: &TerminalPanelEvent, window, cx| match event {
            TerminalPanelEvent::Input(id, input) => {
                this.broadcast_input(CenterTab::Terminal(*id), input, cx);
            }
            TerminalPanelEvent::Activated(terminal_id, _) => {
                this.set_active_tab(Some(CenterTab::Terminal(*terminal_id)), window, cx)
            }
            TerminalPanelEvent::Closed(terminal_id, host_id) => {
                this.cancel_prompts_for_terminal(*terminal_id, window, cx);
                this.terminals.remove(terminal_id);
                this.sync_broadcast(cx);
                this.sync_compose(window, cx);
                if this.active_tab == Some(CenterTab::Terminal(*terminal_id)) {
                    this.set_active_tab(None, window, cx);
                }
                this.forget_unused_temporary_host(*host_id, window, cx);
            }
            TerminalPanelEvent::StatusChanged(..) => {
                this.sync_broadcast(cx);
                this.sync_compose(window, cx);
                cx.notify();
            }
            TerminalPanelEvent::Connected(_, host_id) => {
                this.count_connection(*host_id, cx);
                this.store
                    .update(cx, |store, cx| store.record_connection(*host_id, cx));
            }
            TerminalPanelEvent::PromptRequested(terminal_id, host_id, prompt) => this
                .enqueue_prompt(
                    PromptOwner::Terminal(*terminal_id),
                    *host_id,
                    prompt.clone(),
                    window,
                    cx,
                ),
            TerminalPanelEvent::HostOsDetected(host_id, os) => {
                let (host_id, os) = (*host_id, *os);
                this.store
                    .update(cx, |store, cx| store.set_host_os(host_id, Some(os), cx));
            }
            TerminalPanelEvent::Notice(terminal_id, notice) => {
                this.on_terminal_notice(CenterTab::Terminal(*terminal_id), notice, window, cx)
            }
            TerminalPanelEvent::ZmodemAsk(terminal_id, direction) => {
                this.ask_zmodem(CenterTab::Terminal(*terminal_id), *direction, window, cx)
            }
            TerminalPanelEvent::FilesDropped(terminal_id, files) => {
                this.upload_dropped(*terminal_id, files, window, cx)
            }
        },
    );
    (panel, subscription)
}

struct AuthenticationPromptForm {
    fields: Vec<(ConnectionPromptField, Entity<InputState>)>,
}

impl AuthenticationPromptForm {
    fn new(
        fields: Vec<ConnectionPromptField>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let fields = fields
            .into_iter()
            .map(|field| {
                let echo = field.echo();
                let input = cx.new(|cx| InputState::new(window, cx).masked(!echo));
                (field, input)
            })
            .collect();
        Self { fields }
    }

    fn take_answers(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<ConnectionSecret> {
        self.fields
            .iter()
            .map(|(_, input)| {
                let answer = ConnectionSecret::new(input.read(cx).value().to_string());
                input.update(cx, |input, cx| input.set_value("", window, cx));
                answer
            })
            .collect()
    }
}

impl Render for AuthenticationPromptForm {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Form::new().children(
            self.fields
                .iter()
                .enumerate()
                .map(|(index, (field, input))| {
                    let mut input = Input::new(input).id(("ssh-auth-answer", index)).small();
                    if !field.echo() {
                        input = input.content_type(InputContentType::Password).mask_toggle();
                    }
                    Field::new().label(field.label().to_string()).child(input)
                }),
        )
    }
}

fn new_local_terminal_panel(
    id: LocalTerminalId,
    factory: SharedTerminalTransportFactory,
    name: Option<SavedName>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> (Entity<LocalTerminalPanel>, Subscription) {
    let panel = cx.new(|cx| LocalTerminalPanel::new(id, factory, name, window, cx));
    let subscription = cx.subscribe_in(
        &panel,
        window,
        |this, panel, event: &LocalTerminalPanelEvent, window, cx| match event {
            LocalTerminalPanelEvent::Input(id, input) => {
                this.broadcast_input(CenterTab::LocalTerminal(*id), input, cx);
            }
            LocalTerminalPanelEvent::Activated(id) => {
                this.set_active_tab(Some(CenterTab::LocalTerminal(*id)), window, cx);
                cx.notify();
            }
            LocalTerminalPanelEvent::Closed(id) => {
                this.keep_what_it_shows(*id, panel, cx);
                this.local_terminals.remove(id);
                this.saved_terminals.remove(id);
                if this.front_saved_terminal == Some(*id) {
                    this.front_saved_terminal = None;
                }
                this.sync_broadcast(cx);
                this.sync_compose(window, cx);
                if this.active_tab == Some(CenterTab::LocalTerminal(*id)) {
                    this.set_active_tab(None, window, cx);
                }
                this.sync_saved_terminals(cx);
                cx.notify();
            }
            LocalTerminalPanelEvent::StatusChanged(id) => {
                this.on_local_terminal_retitled(*id, cx);
                this.sync_broadcast(cx);
                this.sync_compose(window, cx);
                cx.notify();
            }
            LocalTerminalPanelEvent::WorkingDirectory(id, folder) => {
                this.on_local_terminal_moved(*id, folder, cx);
            }
            LocalTerminalPanelEvent::Notice(id, notice) => {
                this.on_terminal_notice(CenterTab::LocalTerminal(*id), notice, window, cx)
            }
            LocalTerminalPanelEvent::ZmodemAsk(id, direction) => {
                this.ask_zmodem(CenterTab::LocalTerminal(*id), *direction, window, cx)
            }
        },
    );
    (panel, subscription)
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = match self.active_tab {
            // A terminal tells its own connection, to the host as it took it.
            Some(CenterTab::Terminal(id)) => match self.terminals.get(&id) {
                Some(panel) => {
                    let panel = panel.read(cx);
                    WorkspaceStatus::Host(
                        Some(Box::new((panel.host().clone(), panel.connection_state(cx)))),
                        Some(panel.status(cx)),
                    )
                }
                None => WorkspaceStatus::Host(None, None),
            },
            Some(CenterTab::LocalTerminal(id)) => self
                .local_terminals
                .get(&id)
                .map(|panel| WorkspaceStatus::Local(panel.read(cx).status(cx)))
                .unwrap_or(WorkspaceStatus::Host(None, None)),
            _ => WorkspaceStatus::Host(None, None),
        };
        let sidebar = self.sidebar_showing(cx);
        let running_forwards = self.forwards.read(cx).active_count();
        let tool = self.tool_showing(cx);
        let offered_tools = self.offered_tools(cx);

        div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_new_host))
            .on_action(cx.listener(Self::on_new_temporary_connection))
            .on_action(cx.listener(Self::on_quick_connect))
            .on_action(cx.listener(Self::on_new_local_terminal))
            .on_action(cx.listener(Self::on_upload_files))
            .on_action(cx.listener(Self::on_download_files))
            .on_action(cx.listener(Self::on_start_upload))
            .on_action(cx.listener(Self::on_start_download))
            .on_action(cx.listener(Self::on_cancel_file_transfers))
            .on_action(cx.listener(Self::on_toggle_terminal_broadcast))
            .on_action(cx.listener(Self::on_set_broadcast_target))
            .on_action(cx.listener(Self::on_toggle_compose_pane))
            .on_action(cx.listener(Self::on_send_compose))
            .on_action(cx.listener(Self::on_send_compose_snippet))
            .on_action(cx.listener(Self::on_send_compose_interrupt))
            .on_action(cx.listener(Self::on_toggle_broadcast_keys))
            .on_action(cx.listener(Self::on_edit_host))
            .on_action(cx.listener(Self::on_duplicate_host))
            .on_action(cx.listener(Self::on_move_host_node))
            .on_action(cx.listener(Self::on_delete_host))
            .on_action(cx.listener(Self::on_new_host_in_group))
            .on_action(cx.listener(Self::on_new_group))
            .on_action(cx.listener(Self::on_import_ssh_config))
            .on_action(cx.listener(Self::on_import_known_hosts))
            .on_action(cx.listener(Self::on_expand_all_groups))
            .on_action(cx.listener(Self::on_collapse_all_groups))
            .on_action(cx.listener(Self::on_new_child_group))
            .on_action(cx.listener(Self::on_rename_group))
            .on_action(cx.listener(Self::on_delete_group))
            .on_action(cx.listener(Self::on_connect_host))
            .on_action(cx.listener(Self::on_duplicate_terminal))
            .on_action(cx.listener(Self::on_connect_group))
            .on_action(cx.listener(Self::on_reconnect_terminal))
            .on_action(cx.listener(Self::on_disconnect_terminal))
            .on_action(cx.listener(Self::on_send_break))
            .on_action(cx.listener(Self::on_close_terminal))
            .on_action(cx.listener(Self::on_close_local_terminal))
            .on_action(cx.listener(Self::on_enable_app_lock))
            .on_action(cx.listener(Self::on_change_app_lock_password))
            .on_action(cx.listener(Self::on_disable_app_lock))
            .on_action(cx.listener(Self::on_lock_app))
            .on_action(cx.listener(Self::on_open_settings))
            .on_action(cx.listener(Self::on_check_for_updates))
            .on_action(cx.listener(Self::on_download_update))
            .on_action(cx.listener(Self::on_show_update))
            .on_action(cx.listener(Self::on_restart_to_update))
            .on_action(cx.listener(Self::on_open_download_page))
            .on_action(cx.listener(Self::on_open_changelog))
            .on_action(cx.listener(Self::on_open_website))
            .on_action(cx.listener(Self::on_set_up_sync))
            .on_action(cx.listener(Self::on_set_up_sync_in))
            .on_action(cx.listener(Self::on_set_up_webdav_sync))
            .on_action(cx.listener(Self::on_set_up_s3_sync))
            .on_action(cx.listener(Self::on_change_sync_login))
            .on_action(cx.listener(Self::on_sync_now))
            .on_action(cx.listener(Self::on_review_sync))
            .on_action(cx.listener(Self::on_change_sync_password))
            .on_action(cx.listener(Self::on_turn_off_sync))
            .on_action(cx.listener(Self::on_remove_sync_device))
            .on_action(cx.listener(Self::on_close_settings))
            .on_action(cx.listener(Self::on_close_active_tab))
            .on_action(cx.listener(Self::on_next_tab))
            .on_action(cx.listener(Self::on_previous_tab))
            .on_action(cx.listener(Self::on_switch_to_tab))
            .on_action(cx.listener(Self::on_close_tabs))
            .on_action(cx.listener(Self::on_rename_terminal))
            .on_action(cx.listener(Self::on_copy_host_address))
            .on_action(cx.listener(Self::on_copy_host_id))
            .on_action(cx.listener(Self::on_restart_local_terminal))
            .on_action(cx.listener(Self::on_copy_terminal))
            .on_action(cx.listener(Self::on_paste_terminal))
            .on_action(cx.listener(Self::on_select_all_terminal))
            .on_action(cx.listener(Self::on_find_in_terminal))
            .on_action(cx.listener(Self::on_find_next_in_terminal))
            .on_action(cx.listener(Self::on_find_previous_in_terminal))
            .on_action(cx.listener(Self::on_dismiss_terminal_find))
            .on_action(cx.listener(Self::on_clear_terminal))
            .on_action(cx.listener(Self::on_toggle_host_panel))
            .on_action(cx.listener(Self::on_maximize_window))
            .on_action(cx.listener(Self::on_restore_window))
            .on_action(cx.listener(Self::on_show_or_hide_app))
            .on_action(cx.listener(Self::on_toggle_tool_sidebar))
            .on_action(cx.listener(Self::on_toggle_tool))
            .on_action(cx.listener(Self::on_refresh_gpu))
            .on_action(cx.listener(Self::on_toggle_gpu_device))
            .on_action(cx.listener(Self::on_toggle_all_gpu_devices))
            .on_action(cx.listener(Self::on_refresh_tmux))
            .on_action(cx.listener(Self::on_new_tmux_session))
            .on_action(cx.listener(Self::on_attach_tmux_session))
            .on_action(cx.listener(Self::on_detach_tmux))
            .on_action(cx.listener(Self::on_rename_tmux_session))
            .on_action(cx.listener(Self::on_kill_tmux_session))
            .on_action(cx.listener(Self::on_toggle_monitor_detail))
            .on_action(cx.listener(Self::on_refresh_connections))
            .on_action(cx.listener(Self::on_refresh_processes))
            .on_action(cx.listener(Self::on_sort_processes))
            .on_action(cx.listener(Self::on_end_process))
            .on_action(cx.listener(Self::on_show_process))
            .on_action(cx.listener(Self::on_refresh_services))
            .on_action(cx.listener(Self::on_show_service))
            .on_action(cx.listener(Self::on_control_service))
            .on_action(cx.listener(Self::on_refresh_history))
            .on_action(cx.listener(Self::on_enter_command))
            .on_action(cx.listener(Self::on_copy_command))
            .on_action(cx.listener(Self::on_new_terminal_group))
            .on_action(cx.listener(Self::on_rename_terminal_group))
            .on_action(cx.listener(Self::on_delete_terminal_group))
            .on_action(cx.listener(Self::on_new_saved_terminal))
            .on_action(cx.listener(Self::on_open_saved_terminal))
            .on_action(cx.listener(Self::on_close_saved_terminal))
            .on_action(cx.listener(Self::on_rename_saved_terminal))
            .on_action(cx.listener(Self::on_delete_saved_terminal))
            .on_action(cx.listener(Self::on_move_terminal_node))
            .on_action(cx.listener(Self::on_toggle_terminal_group))
            .on_action(cx.listener(Self::on_new_snippet))
            .on_action(cx.listener(Self::on_new_snippet_in))
            .on_action(cx.listener(Self::on_edit_snippet))
            .on_action(cx.listener(Self::on_delete_snippet))
            .on_action(cx.listener(Self::on_new_snippet_category))
            .on_action(cx.listener(Self::on_rename_snippet_category))
            .on_action(cx.listener(Self::on_delete_snippet_category))
            .on_action(cx.listener(Self::on_toggle_snippet_category))
            .on_action(cx.listener(Self::on_new_snippet_subcategory))
            .on_action(cx.listener(Self::on_move_snippet_node))
            .on_action(cx.listener(Self::on_refresh_docker))
            .on_action(cx.listener(Self::on_kubernetes_action))
            .on_action(cx.listener(Self::on_toggle_docker_project))
            .on_action(cx.listener(Self::on_show_docker_object))
            .on_action(cx.listener(Self::on_control_containers))
            .on_action(cx.listener(Self::on_remove_docker_object))
            .on_action(cx.listener(Self::on_show_hosts))
            .on_action(cx.listener(Self::on_show_forwards))
            .on_action(cx.listener(Self::on_new_forward))
            .on_action(cx.listener(Self::on_edit_forward))
            .on_action(cx.listener(Self::on_delete_forward))
            .on_action(cx.listener(Self::on_start_forward))
            .on_action(cx.listener(Self::on_stop_forward))
            .on_action(cx.listener(Self::on_show_credentials))
            .on_action(cx.listener(Self::on_new_credential))
            .on_action(cx.listener(Self::on_generate_credential_key))
            .on_action(cx.listener(Self::on_edit_credential))
            .on_action(cx.listener(Self::on_copy_credential_public_key))
            .on_action(cx.listener(Self::on_delete_credential))
            .on_action(cx.listener(Self::on_toggle_theme))
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_zoom_in))
            .on_action(cx.listener(Self::on_zoom_out))
            .on_action(cx.listener(Self::on_zoom_reset))
            .on_action(cx.listener(Self::on_zoom_terminal_in))
            .on_action(cx.listener(Self::on_zoom_terminal_out))
            .on_action(cx.listener(Self::on_zoom_terminal_reset))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(render_title_bar(
                sidebar,
                running_forwards,
                self.updater.read(cx).snapshot().badge(),
                &self.switch_end,
                cx.entity_id(),
                &self.focus_handle,
                cx,
            ))
            .child(render_sidebar_toggle(
                sidebar.is_some(),
                self.dock_area
                    .read(cx)
                    .dock_size(DockPlacement::Left)
                    .unwrap_or_default(),
                &self.switch_end,
                &self.focus_handle,
            ))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(self.dock_area.clone()),
                    )
                    // The switch comes and goes with the SSH terminals.
                    .when(!offered_tools.is_empty(), |area| {
                        area.child(render_tool_switch(
                            tool,
                            &offered_tools,
                            &self.focus_handle,
                            cx,
                        ))
                    }),
            )
            .child(WorkspaceStatusBar {
                status,
                transfers: self.transfers.read(cx).status(),
                dispatch: self.focus_handle.clone(),
            })
            .children(crate::shared::form_error_layer(cx))
    }
}
