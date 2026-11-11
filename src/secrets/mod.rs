//! Where ShellRS keeps passwords and passphrases: the interface and the
//! stores that keep nothing on disk are the hosts core's; the one in use
//! is the vault's sealed file ([`crate::vault::VaultSecretStore`]).

pub use shellrs_hosts::secrets::*;
