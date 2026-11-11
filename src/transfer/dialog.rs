//! The two questions of a file transfer: where on the host an upload goes,
//! and what to download and into which folder.

use std::path::PathBuf;

use gpui_kit::component::{
    Sizable as _, WindowExt as _,
    button::Button,
    form::{Field, Form},
    h_flex,
    input::{Input, InputState},
};
use gpui_kit::*;

use crate::app::{StartDownload, StartUpload};
use crate::i18n::{t, tn};
use crate::shared::{commit_footer, dismiss_form_error, show_form_error};
use crate::terminal::RemoteTerminalId;

struct UploadForm {
    terminal: RemoteTerminalId,
    sources: Vec<PathBuf>,
    destination: Entity<InputState>,
    dispatch: FocusHandle,
}

impl UploadForm {
    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let action = StartUpload {
            terminal: self.terminal,
            sources: self.sources.clone(),
            destination: self.destination.read(cx).value().trim().to_string(),
        };
        // The dialog's focus goes with it: through the workspace's.
        let dispatch = self.dispatch.clone();
        window.defer(cx, move |window, cx| {
            dispatch.dispatch_action(&action, window, cx)
        });
        true
    }
}

impl Render for UploadForm {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let what = match self.sources.as_slice() {
            [one] => one.display().to_string(),
            sources => tn!("transfer.upload.items", sources.len()).to_string(),
        };
        Form::new()
            .child(
                Field::new().label(t!("transfer.upload.what")).child(
                    div()
                        .id("upload-sources")
                        .test_support()
                        .aria_label(what.clone())
                        .text_sm()
                        .child(what),
                ),
            )
            .child(
                Field::new()
                    .label(t!("transfer.upload.destination"))
                    .description(t!("transfer.upload.destination_description"))
                    .child(
                        Input::new(&self.destination)
                            .id("upload-destination")
                            .small(),
                    ),
            )
    }
}

/// Ask which folder of the host `sources` go into, starting from
/// `folder`, the terminal's own when its shell says, and start the upload.
pub fn open_upload_dialog(
    terminal: RemoteTerminalId,
    host: SharedString,
    sources: Vec<PathBuf>,
    folder: Option<String>,
    dispatch: FocusHandle,
    window: &mut Window,
    cx: &mut App,
) {
    let form = cx.new(|cx| UploadForm {
        terminal,
        sources,
        destination: cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("~")
                .default_value(folder.unwrap_or_default())
        }),
        dispatch,
    });
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(t!("transfer.upload.title", host = host))
            // Closed by its buttons or Escape, not by a click beside it.
            .overlay_closable(false)
            .child(form.clone())
            .footer(commit_footer("commit", t!("transfer.upload.commit")))
            .on_ok({
                let form = form.clone();
                move |_, window, cx| form.update(cx, |form, cx| form.commit(window, cx))
            })
    });
}

struct DownloadForm {
    terminal: RemoteTerminalId,
    source: Entity<InputState>,
    into: Option<PathBuf>,
    dispatch: FocusHandle,
}

impl DownloadForm {
    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let source = self.source.read(cx).value().trim().to_string();
        let error = if source.is_empty() {
            Some(t!("transfer.download.enter_path"))
        } else if self.into.is_none() {
            Some(t!("transfer.download.choose_folder"))
        } else {
            None
        };
        let (None, Some(into)) = (&error, self.into.clone()) else {
            let error = error.unwrap_or_else(|| t!("transfer.download.choose_folder"));
            show_form_error(error, window, cx);
            return false;
        };
        let action = StartDownload {
            terminal: self.terminal,
            source,
            into,
        };
        // The dialog's focus goes with it: through the workspace's.
        let dispatch = self.dispatch.clone();
        window.defer(cx, move |window, cx| {
            dispatch.dispatch_action(&action, window, cx)
        });
        true
    }

    /// The system's picker for the folder the download goes into.
    fn choose_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t!("transfer.download.choose")),
        });
        cx.spawn_in(window, async move |this, cx| {
            let folder = chosen
                .await
                .ok()
                .and_then(Result::ok)
                .flatten()
                .and_then(|paths| paths.into_iter().next());
            if let Some(folder) = folder {
                let _ = this.update(cx, |this, cx| {
                    this.into = Some(folder);
                    cx.notify();
                });
            }
        })
        .detach();
    }
}

impl Render for DownloadForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let into = self
            .into
            .as_ref()
            .map(|folder| folder.display().to_string())
            .unwrap_or_default();
        Form::new()
            .child(
                Field::new()
                    .label(t!("transfer.download.source"))
                    .description(t!("transfer.download.source_description"))
                    .required(true)
                    .child(Input::new(&self.source).id("download-source").small()),
            )
            .child(
                Field::new().label(t!("transfer.download.into")).child(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .id("download-into")
                                .test_support()
                                .aria_label(into.clone())
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .truncate()
                                .child(into),
                        )
                        .child(
                            Button::new("download-choose")
                                .small()
                                .outline()
                                .label(t!("transfer.download.choose"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.choose_folder(window, cx)
                                })),
                        ),
                ),
            )
    }
}

/// Ask what to copy from the host and into which folder of this computer,
/// starting from `folder`, the terminal's own when its shell says, and
/// `into`, and start the download.
pub fn open_download_dialog(
    terminal: RemoteTerminalId,
    host: SharedString,
    folder: Option<String>,
    into: Option<PathBuf>,
    dispatch: FocusHandle,
    window: &mut Window,
    cx: &mut App,
) {
    let form = cx.new(|cx| DownloadForm {
        terminal,
        source: cx.new(|cx| {
            let start = folder.map(|folder| match folder.ends_with('/') {
                true => folder,
                false => format!("{folder}/"),
            });
            InputState::new(window, cx)
                .placeholder("/var/log/syslog")
                .default_value(start.unwrap_or_default())
        }),
        into,
        dispatch,
    });
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(t!("transfer.download.title", host = host))
            // Closed by its buttons or Escape, not by a click beside it.
            .overlay_closable(false)
            .child(form.clone())
            .footer(commit_footer("commit", t!("transfer.download.commit")))
            .on_ok({
                let form = form.clone();
                move |_, window, cx| form.update(cx, |form, cx| form.commit(window, cx))
            })
            .on_close(|_, window, cx| dismiss_form_error(window, cx))
    });
}
