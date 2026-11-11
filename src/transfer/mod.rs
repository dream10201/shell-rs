//! 文件传输: copying files to and from an SSH host over a remote
//! terminal's own connection, with nothing but the host's shell (see
//! `shellrs_ssh::transfer`): no login of its own, no SFTP on the server.

mod describe;
mod dialog;
mod manager;

pub use describe::describe_transfer_error;
pub use dialog::{open_download_dialog, open_upload_dialog};
pub use manager::{TransferEvent, TransferJob, TransferManager, TransferStatus};
