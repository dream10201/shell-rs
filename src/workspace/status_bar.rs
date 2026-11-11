use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    separator::Separator,
    status_bar::StatusBar,
    tooltip::Tooltip,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::app::{CancelFileTransfers, CatalogIcon};
use crate::host::{ConnectionState, Host};
use crate::i18n::{t, tn};
use crate::terminal::{TerminalLifecycle, TerminalStatus};
use crate::transfer::TransferStatus;

/// The window's status bar: the tab in front, and the file transfers that
/// are running, with the button that stops them.
#[derive(IntoElement)]
pub struct WorkspaceStatusBar {
    pub status: WorkspaceStatus,
    pub transfers: Option<TransferStatus>,
    /// Where the button's action goes: the workspace.
    pub dispatch: FocusHandle,
}

impl RenderOnce for WorkspaceStatusBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let bar = self.status.bar(cx);
        let Some(transfers) = self.transfers else {
            return bar;
        };
        let mut text = match (&transfers.file, transfers.upload) {
            (Some((name, percent)), true) => {
                t!("transfer.status.uploading", name = name, percent = percent)
            }
            (Some((name, percent)), false) => {
                t!(
                    "transfer.status.downloading",
                    name = name,
                    percent = percent
                )
            }
            (None, true) => t!("transfer.status.starting_upload"),
            (None, false) => t!("transfer.status.starting_download"),
        }
        .to_string();
        if transfers.running > 1 {
            text = format!(
                "{text} · {}",
                tn!("transfer.status.more", transfers.running - 1)
            );
        }
        let dispatch = self.dispatch;
        bar.left(Separator::vertical()).left(
            h_flex()
                .gap_1()
                .child(
                    div()
                        .id("status-transfers")
                        .test_support()
                        .aria_label(text.clone())
                        .child(text),
                )
                .child(
                    Button::new("cancel-transfers")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .tooltip(t!("transfer.status.cancel"))
                        .on_click(move |_, window, cx| {
                            dispatch.dispatch_action(&CancelFileTransfers, window, cx)
                        }),
                ),
        )
    }
}

/// The active connection/process state on the left and terminal facts on the
/// right, ending in the size of the terminal in front, local or remote. With
/// no terminal in front there is no size to show.
pub enum WorkspaceStatus {
    /// The remote terminal in front: its host as it took it, its own
    /// connection, and its state and size. The start page and the
    /// settings have none.
    Host(Option<Box<(Host, ConnectionState)>>, Option<TerminalStatus>),
    Local(TerminalStatus),
}

/// The icon for a connection state; a dropped one, like a problem, in red.
fn state_icon(state: ConnectionState, cx: &App) -> Icon {
    match state {
        ConnectionState::Connected => {
            Icon::new(IconName::CircleCheck).text_color(cx.theme().success)
        }
        ConnectionState::Connecting => {
            Icon::new(IconName::LoaderCircle).text_color(cx.theme().warning)
        }
        ConnectionState::Disconnected => {
            Icon::new(CatalogIcon::Unplug).text_color(cx.theme().danger)
        }
    }
}

/// 「已连接 web-01」: where the connection to `name` stands.
fn connection_text(state: ConnectionState, name: &str) -> SharedString {
    match state {
        ConnectionState::Disconnected => t!("workspace.status.disconnected_from", name = name),
        ConnectionState::Connecting => t!("workspace.status.connecting_to", name = name),
        ConnectionState::Connected => t!("workspace.status.connected_to", name = name),
    }
}

impl WorkspaceStatus {
    fn bar(self, cx: &mut App) -> StatusBar {
        let muted = cx.theme().muted_foreground;
        // Red when something is wrong: a dropped connection.
        let mut alarming = false;
        let (text, icon, address, terminal): (
            SharedString,
            Icon,
            Option<String>,
            Option<TerminalStatus>,
        ) = match self {
            WorkspaceStatus::Host(active, terminal) => match active {
                Some(active) => {
                    let (host, state) = *active;
                    alarming = state == ConnectionState::Disconnected;
                    (
                        connection_text(state, &host.name),
                        state_icon(state, cx),
                        Some(host.endpoint()),
                        terminal,
                    )
                }
                None => (
                    ConnectionState::Disconnected.label(),
                    Icon::new(CatalogIcon::Unplug).text_color(muted),
                    None,
                    terminal,
                ),
            },
            WorkspaceStatus::Local(status) => {
                let lifecycle = status.lifecycle();
                let icon = match lifecycle {
                    TerminalLifecycle::Running => {
                        Icon::new(IconName::CircleCheck).text_color(cx.theme().success)
                    }
                    TerminalLifecycle::Starting => {
                        Icon::new(IconName::LoaderCircle).text_color(cx.theme().warning)
                    }
                    TerminalLifecycle::Exited { .. }
                    | TerminalLifecycle::Failed(_)
                    | TerminalLifecycle::Closing => {
                        Icon::new(CatalogIcon::Terminal).text_color(muted)
                    }
                };
                (
                    match lifecycle {
                        TerminalLifecycle::Starting => t!("workspace.status.local_starting"),
                        TerminalLifecycle::Running => t!("workspace.status.local_running"),
                        TerminalLifecycle::Exited { .. } => t!("workspace.status.local_exited"),
                        TerminalLifecycle::Failed(_) => t!("workspace.status.local_failed"),
                        TerminalLifecycle::Closing => t!("workspace.status.local_closing"),
                    },
                    icon,
                    None,
                    Some(status),
                )
            }
        };

        let danger = cx.theme().danger;
        StatusBar::new()
            .left(
                div()
                    .id("status-connection")
                    .test_support()
                    .aria_label(text.clone())
                    .child(
                        h_flex()
                            .gap_1()
                            .when(alarming, |this| this.text_color(danger))
                            .child(icon.small())
                            .child(text),
                    ),
            )
            .when_some(address, |bar, address| {
                bar.left(Separator::vertical()).left(address)
            })
            .right(t!("workspace.status.encoding"))
            .right(Separator::vertical())
            .right(t!("workspace.status.terminal_type"))
            .when_some(terminal, |bar, terminal| {
                let (columns, rows) = (terminal.columns(), terminal.rows());
                let size = format!("{columns}×{rows}");
                let explained = t!(
                    "workspace.status.terminal_size",
                    columns = columns,
                    rows = rows
                );
                bar.right(Separator::vertical()).right(
                    div()
                        .id("status-terminal-size")
                        .test_support()
                        .aria_label(size.clone())
                        .tooltip(move |window, cx| {
                            Tooltip::new(explained.clone()).build(window, cx)
                        })
                        .child(size),
                )
            })
    }
}
