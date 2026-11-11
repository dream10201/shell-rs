//! One ShellRS per data directory. The running app listens on a local
//! socket; opened again, ShellRS asks the one running to come forward and
//! hands it the link it was opened with (外部连接), then leaves.

mod client;
mod link;
#[cfg(windows)]
mod pipe_windows;
mod protocol;
mod server;

use std::ffi::OsString;

pub use client::activate_running_app;
pub use link::{OpenLink, link_arguments};
pub use server::InstanceServer;

/// The arguments the process was started with, apart from the one macOS
/// adds when launching an app.
pub fn command_line_arguments() -> Vec<OsString> {
    std::env::args_os()
        .skip(1)
        .filter(|arg| !arg.to_string_lossy().starts_with("-psn_"))
        .collect()
}
