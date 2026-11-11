//! The workspace's share of the links ShellRS is opened with (外部连接):
//! a bastion host runs `ShellRS ssh://user@host:port`, the way it runs
//! Xshell, or `ShellRS sftp://…`, the way it runs WinSCP, and ShellRS
//! connects without saving the host. The host goes with its last tab.

use gpui_kit::component::{WindowExt as _, dock::DockPlacement, notification::Notification};
use gpui_kit::*;
use std::time::Duration;

use crate::app::ShowOrHideApp;
use crate::host::{HostId, LinkKind, SshLink};
use crate::i18n::t;
use crate::instance::{InstanceServer, OpenLink};

use super::workspace_view::Workspace;

/// How often the app looks for what the instance server has heard.
const REOPEN_POLL: Duration = Duration::from_millis(100);

impl Workspace {
    /// Bring the window forward whenever ShellRS is opened while it is
    /// already running, and open the link it was opened with. The second
    /// copy of ShellRS only passes the word on and exits: two of them on
    /// one data directory would each keep their own copy of the hosts and
    /// write over the other's changes.
    ///
    /// Asked on a timer, like every other worker: the thread that hears the
    /// request never wakes the window itself. 显示或隐藏 ShellRS's system
    /// key is asked for on the same timer.
    pub(super) fn serve_reopening(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(REOPEN_POLL).await;
                let served = this.update_in(cx, |this, window, cx| {
                    if crate::app::take_system_hotkey_press() {
                        this.on_show_or_hide_app(&ShowOrHideApp, window, cx);
                    }
                    let Some(links) = this
                        .instance_server
                        .as_ref()
                        .and_then(InstanceServer::take_activation)
                    else {
                        return;
                    };
                    crate::app::bring_forward(window, cx);
                    for link in links {
                        this.open_link(link, window, cx);
                    }
                });
                if served.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Connect to what `link` names in a tab of its own across the window,
    /// a terminal or an SFTP tab by the link's scheme, or say what is wrong
    /// with it.
    pub fn open_link(&mut self, link: OpenLink, window: &mut Window, cx: &mut Context<Self>) {
        if crate::app_lock::is_locked(cx) {
            self.locked_links.push(link);
            return;
        }
        match SshLink::parse(&link.url, link.tab.as_deref()) {
            Ok(link) => {
                let kind = link.kind;
                let host = self
                    .store
                    .update(cx, |store, cx| store.insert_external(link, cx));
                match kind {
                    LinkKind::Ssh => self.connect_host(host, window, cx),
                    LinkKind::Sftp => self.open_explorer(host, window, cx),
                }
                // The tab is what the bastion host opened ShellRS for: the
                // hosts make way, as with ⌘B, which brings them back.
                if self.dock_area.read(cx).is_dock_open(DockPlacement::Left) {
                    self.dock_area.update(cx, |area, cx| {
                        area.toggle_dock(DockPlacement::Left, window, cx);
                    });
                }
            }
            Err(problem) => window.push_notification(
                Notification::error(problem).title(t!("workspace.link.open_failed")),
                cx,
            ),
        }
    }

    /// Forget a temporary host once its last tab has closed, with whatever
    /// it was still asking.
    pub(super) fn forget_unused_temporary_host(
        &mut self,
        host: HostId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.store.read(cx).is_temporary(host) || self.has_tabs(host, cx) {
            return;
        }
        self.cancel_prompts_for_host(host, window, cx);
        self.store
            .update(cx, |store, cx| store.remove_temporary(host, cx));
    }
}

/// Open the link ShellRS was started with, once the window is there: an
/// error notification needs the window's `Root`, which is only in place
/// after the closure building the window returns (see
/// [`super::notify_once_open`]).
pub fn open_link_once_open(
    workspace: &Entity<Workspace>,
    link: OpenLink,
    window: &mut Window,
    cx: &mut App,
) {
    let workspace = workspace.downgrade();
    window.defer(cx, move |window, cx| {
        workspace
            .update(cx, |workspace, cx| workspace.open_link(link, window, cx))
            .ok();
    });
}
