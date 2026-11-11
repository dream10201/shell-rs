//! The workspace's share of 文件传输: the two commands of a remote
//! terminal, what their dialogs start, and what is said when a transfer
//! ends.

use gpui_kit::component::{WindowExt as _, notification::Notification};
use gpui_kit::*;

use super::Workspace;
use crate::app::{CancelFileTransfers, DownloadFiles, StartDownload, StartUpload, UploadFiles};
use crate::i18n::{t, tn};
use crate::terminal::{RemoteTerminalId, TerminalChannels, TerminalLifecycle};
use crate::transfer::{TransferEvent, TransferJob, open_download_dialog, open_upload_dialog};

/// What a transfer needs of the terminal it goes over.
struct TransferTerminal {
    host: SharedString,
    channels: TerminalChannels,
    /// The folder its shell last reported, if it has.
    folder: Option<String>,
}

impl Workspace {
    /// A remote terminal whose connection can carry a transfer: an SSH
    /// one, connected.
    fn transfer_terminal(&self, id: RemoteTerminalId, cx: &App) -> Option<TransferTerminal> {
        let panel = self.terminals.get(&id)?.read(cx);
        if !panel.host().is_ssh() || panel.lifecycle(cx) != TerminalLifecycle::Running {
            return None;
        }
        let terminal = panel.terminal().read(cx);
        Some(TransferTerminal {
            host: panel.host().name.clone(),
            channels: terminal.channels(cx),
            folder: terminal.directory(cx).map(|directory| directory.path),
        })
    }

    /// 上传文件…: the system's picker, then where on the host they go.
    pub(super) fn on_upload_files(
        &mut self,
        action: &UploadFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = action.0;
        if self.transfer_terminal(id, cx).is_none() {
            return;
        }
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some(t!("transfer.upload.choose")),
        });
        cx.spawn_in(window, async move |this, cx| {
            let sources = chosen
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .unwrap_or_default();
            if sources.is_empty() {
                return;
            }
            let _ = this.update_in(cx, |this, window, cx| {
                // The terminal may have gone meanwhile.
                let Some(terminal) = this.transfer_terminal(id, cx) else {
                    return;
                };
                open_upload_dialog(
                    id,
                    terminal.host,
                    sources,
                    terminal.folder,
                    this.focus_handle.clone(),
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    /// 下载文件…: what to copy, and into which folder of this computer.
    pub(super) fn on_download_files(
        &mut self,
        action: &DownloadFiles,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal) = self.transfer_terminal(action.0, cx) else {
            return;
        };
        open_download_dialog(
            action.0,
            terminal.host,
            terminal.folder,
            dirs::download_dir(),
            self.focus_handle.clone(),
            window,
            cx,
        );
    }

    pub(super) fn on_start_upload(
        &mut self,
        action: &StartUpload,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let job = TransferJob::Upload {
            sources: action.sources.clone(),
            destination: action.destination.clone(),
        };
        self.start_transfer(action.terminal, job, window, cx);
    }

    pub(super) fn on_start_download(
        &mut self,
        action: &StartDownload,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let job = TransferJob::Download {
            source: action.source.clone(),
            into: action.into.clone(),
        };
        self.start_transfer(action.terminal, job, window, cx);
    }

    fn start_transfer(
        &mut self,
        id: RemoteTerminalId,
        job: TransferJob,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal) = self.transfer_terminal(id, cx) else {
            window.push_notification(Notification::error(t!("transfer.error.not_connected")), cx);
            return;
        };
        self.transfers.update(cx, |transfers, cx| {
            transfers.start(terminal.channels, terminal.host, job, cx)
        });
    }

    pub(super) fn on_cancel_file_transfers(
        &mut self,
        _: &CancelFileTransfers,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.transfers
            .update(cx, |transfers, _| transfers.cancel_all());
    }

    pub(super) fn on_transfer_event(
        &mut self,
        event: &TransferEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let TransferEvent::Finished { host, job, outcome } = event;
        let notification = match (outcome, job) {
            (Some(Ok(summary)), TransferJob::Upload { .. }) => Notification::success(tn!(
                "transfer.done.uploaded",
                summary.files as usize,
                host = host
            )),
            (Some(Ok(summary)), TransferJob::Download { into, .. }) => Notification::success(tn!(
                "transfer.done.downloaded",
                summary.files as usize,
                folder = into.display()
            )),
            (Some(Err(reason)), job) => {
                Notification::error(reason.clone()).title(match job.is_upload() {
                    true => t!("transfer.failed.upload", host = host),
                    false => t!("transfer.failed.download", host = host),
                })
            }
            (None, _) => Notification::info(t!("transfer.cancelled")),
        };
        window.push_notification(notification, cx);
    }
}
