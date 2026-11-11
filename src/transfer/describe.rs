//! Why a file transfer failed, in words.

use std::error::Error;

use crate::i18n::t;
use crate::ssh::link_as;
use crate::ssh::transfer::TransferFailure;

/// A command of a transfer that could not be started: the terminal has no
/// connection to start it on, or the server or the terminal said why not.
#[derive(Debug)]
pub(super) struct NotStarted(pub Option<String>);

impl std::fmt::Display for NotStarted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(reason) => f.write_str(reason),
            None => f.write_str("the terminal isn't connected"),
        }
    }
}

impl Error for NotStarted {}

/// What went wrong, down to the cause.
pub fn describe_transfer_error(error: &anyhow::Error) -> String {
    let causes: Vec<&(dyn Error + 'static)> = error.chain().collect();
    let under = || {
        causes
            .iter()
            .skip(1)
            .map(|cause| cause.to_string())
            .collect::<Vec<_>>()
            .join(": ")
    };
    if let Some(NotStarted(reason)) = causes[0].downcast_ref::<NotStarted>() {
        return match reason {
            Some(reason) => reason.clone(),
            None => t!("transfer.error.not_connected").to_string(),
        };
    }
    let Some(failure) = link_as::<TransferFailure>(causes[0]) else {
        return error.to_string();
    };
    match failure {
        TransferFailure::Remote(message) if message.is_empty() => t!("transfer.error.remote"),
        TransferFailure::Remote(message) => {
            t!("transfer.error.remote_said", message = message)
        }
        TransferFailure::NotFound(path) => t!("transfer.error.not_found", path = path),
        TransferFailure::Unexpected => t!("transfer.error.unexpected"),
        TransferFailure::UnsafeName(name) => t!("transfer.error.unsafe_name", name = name),
        TransferFailure::UnsendableName(path) => {
            t!("transfer.error.unsendable_name", path = path.display())
        }
        TransferFailure::Local(path) => {
            t!(
                "transfer.error.local",
                path = path.display(),
                error = under()
            )
        }
        TransferFailure::Connection => t!("transfer.error.connection", error = under()),
        TransferFailure::Cancelled => t!("transfer.error.cancelled"),
    }
    .to_string()
}
