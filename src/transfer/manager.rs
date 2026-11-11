//! The file transfers that are running. Each has a worker thread of its
//! own, which starts the transfer's commands on a remote terminal's
//! connection; the manager collects what the workers say on a timer.

use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use futures::future::BoxFuture;
use gpui_kit::*;
use tokio::sync::watch;

use super::describe::{NotStarted, describe_transfer_error};
use crate::ssh::transfer::{
    self, CommandRunner, RunningCommand, TransferProgress, TransferSummary,
};
use crate::terminal::TerminalChannels;

/// How often the workers' events are collected.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// How often a worker says how far it has come.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// What to copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferJob {
    /// Files and folders of this computer, into a folder of the host.
    Upload {
        sources: Vec<PathBuf>,
        destination: String,
    },
    /// A file or folder of the host, into a folder of this computer.
    Download { source: String, into: PathBuf },
}

impl TransferJob {
    pub fn is_upload(&self) -> bool {
        matches!(self, Self::Upload { .. })
    }
}

/// A transfer that ended.
#[derive(Clone, Debug)]
pub enum TransferEvent {
    Finished {
        host: SharedString,
        job: TransferJob,
        /// What it moved, or why it stopped; `None` when it was cancelled.
        outcome: Option<Result<TransferSummary, String>>,
    },
}

/// What the status bar says of the transfers that are running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferStatus {
    pub running: usize,
    /// The first one's direction, file and how far that file has come, in
    /// percent; no file before its first one starts.
    pub upload: bool,
    pub file: Option<(String, u64)>,
}

enum WorkerEvent {
    Progress(TransferProgress),
    Finished(Result<TransferSummary>),
}

struct Running {
    host: SharedString,
    job: TransferJob,
    cancel: watch::Sender<bool>,
    events: mpsc::Receiver<WorkerEvent>,
    progress: Option<TransferProgress>,
}

#[derive(Default)]
pub struct TransferManager {
    running: Vec<Running>,
    polling: bool,
}

impl EventEmitter<TransferEvent> for TransferManager {}

impl TransferManager {
    /// Start `job` with the host called `host`, on the connection of the
    /// terminal whose channels these are.
    pub fn start(
        &mut self,
        channels: TerminalChannels,
        host: SharedString,
        job: TransferJob,
        cx: &mut Context<Self>,
    ) {
        let (cancel, cancelled) = watch::channel(false);
        let (sender, events) = mpsc::channel();
        let work = job.clone();
        let spawned = std::thread::Builder::new()
            .name("shellrs-transfer".into())
            .spawn(move || {
                let result = run(channels, work, cancelled, &sender);
                let _ = sender.send(WorkerEvent::Finished(result));
            });
        if let Err(error) = spawned {
            cx.emit(TransferEvent::Finished {
                host,
                job,
                outcome: Some(Err(error.to_string())),
            });
            return;
        }
        self.running.push(Running {
            host,
            job,
            cancel,
            events,
            progress: None,
        });
        self.poll_while_running(cx);
        cx.notify();
    }

    /// How many transfers are running.
    pub fn running(&self) -> usize {
        self.running.len()
    }

    pub fn status(&self) -> Option<TransferStatus> {
        let first = self.running.first()?;
        Some(TransferStatus {
            running: self.running.len(),
            upload: first.job.is_upload(),
            file: first.progress.as_ref().map(|progress| {
                let percent = (progress.done.min(progress.total).saturating_mul(100))
                    .checked_div(progress.total)
                    .unwrap_or(100);
                (progress.name.clone(), percent)
            }),
        })
    }

    /// Stop every transfer; each ends as cancelled once its worker hears.
    pub fn cancel_all(&mut self) {
        for running in &self.running {
            let _ = running.cancel.send(true);
        }
    }

    /// Collect the workers' events on a timer for as long as one is running.
    fn poll_while_running(&mut self, cx: &mut Context<Self>) {
        if self.polling {
            return;
        }
        self.polling = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let running = this.update(cx, |this, cx| {
                    this.collect_events(cx);
                    this.polling = !this.running.is_empty();
                    this.polling
                });
                if !matches!(running, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    fn collect_events(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        let mut ix = 0;
        while ix < self.running.len() {
            let mut finished = None;
            loop {
                match self.running[ix].events.try_recv() {
                    Ok(WorkerEvent::Progress(progress)) => {
                        self.running[ix].progress = Some(progress);
                        changed = true;
                    }
                    Ok(WorkerEvent::Finished(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    // The worker is gone without a last word.
                    Err(TryRecvError::Disconnected) => {
                        finished = Some(Err(anyhow!(NotStarted(None))));
                        break;
                    }
                }
            }
            let Some(result) = finished else {
                ix += 1;
                continue;
            };
            let running = self.running.remove(ix);
            let cancelled = *running.cancel.borrow();
            let outcome = match result {
                Ok(summary) => Some(Ok(summary)),
                Err(_) if cancelled => None,
                Err(error) => Some(Err(describe_transfer_error(&error))),
            };
            cx.emit(TransferEvent::Finished {
                host: running.host,
                job: running.job,
                outcome,
            });
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }
}

/// Starts a transfer's commands on a terminal's connection.
struct TerminalRunner(TerminalChannels);

impl CommandRunner for TerminalRunner {
    fn start(&mut self, command: String) -> BoxFuture<'_, Result<Box<dyn RunningCommand>>> {
        let answer = self.0.start_command(command);
        Box::pin(async move {
            match answer.await {
                Ok(Ok(command)) => Ok(command),
                Ok(Err(reason)) => Err(anyhow!(NotStarted(Some(reason)))),
                // Nothing took the request: the terminal has no connection.
                Err(_) => Err(anyhow!(NotStarted(None))),
            }
        })
    }
}

/// A transfer from its first command to its last, on the worker's thread.
fn run(
    channels: TerminalChannels,
    job: TransferJob,
    cancelled: watch::Receiver<bool>,
    events: &mpsc::Sender<WorkerEvent>,
) -> Result<TransferSummary> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut runner = TerminalRunner(channels);
    let mut last: Option<(Instant, u64)> = None;
    let mut progress = |progress: TransferProgress| {
        // A file's first and last word always, the rest now and then.
        let ends = progress.done == 0 || progress.done == progress.total;
        let due = last
            .is_none_or(|(at, files)| files != progress.files || at.elapsed() >= PROGRESS_INTERVAL);
        if ends || due {
            last = Some((Instant::now(), progress.files));
            let _ = events.send(WorkerEvent::Progress(progress));
        }
    };
    runtime.block_on(async {
        match &job {
            TransferJob::Upload {
                sources,
                destination,
            } => {
                transfer::upload(&mut runner, sources, destination, &mut progress, &cancelled).await
            }
            TransferJob::Download { source, into } => {
                transfer::download(&mut runner, source, into, &mut progress, &cancelled).await
            }
        }
    })
}
