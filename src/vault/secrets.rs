//! The passwords and passphrases ShellRS saves, in one sealed file of the
//! vault. Nothing of it stays in memory between two calls.

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use shellrs_hosts::secrets::{SecretRef, SecretStore};
use shellrs_ssh::files::Files as _;
use zeroize::{Zeroize as _, Zeroizing};

use super::Vault;

const FILE: &str = "secrets";

pub struct VaultSecretStore {
    vault: Arc<Vault>,
    path: PathBuf,
    /// One change at a time: each reads the file, changes it and writes it.
    changing: Mutex<()>,
}

/// The file's content: each secret by its account. Wiped when dropped.
#[derive(Default)]
struct Secrets(BTreeMap<String, String>);

impl Drop for Secrets {
    fn drop(&mut self) {
        for value in self.0.values_mut() {
            value.zeroize();
        }
    }
}

impl VaultSecretStore {
    pub fn new(vault: Arc<Vault>) -> Self {
        let path = vault.dir().join(FILE);
        Self {
            vault,
            path,
            changing: Mutex::new(()),
        }
    }

    fn read(&self) -> Result<Secrets> {
        let bytes = match self.vault.read(&self.path) {
            Ok(bytes) => Zeroizing::new(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Secrets::default()),
            Err(error) => return Err(error.into()),
        };
        Ok(Secrets(serde_json::from_slice(&bytes)?))
    }

    fn change(&self, change: impl FnOnce(&mut BTreeMap<String, String>)) -> Result<()> {
        let _changing = self
            .changing
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let mut secrets = self.read()?;
        change(&mut secrets.0);
        // Room for all of it at once: a buffer that grows leaves copies
        // of its content behind.
        let room: usize = secrets
            .0
            .iter()
            .map(|(account, value)| 2 * (account.len() + value.len()) + 8)
            .sum();
        let mut json = Zeroizing::new(Vec::with_capacity(room + 2));
        serde_json::to_writer(&mut *json, &secrets.0)?;
        Ok(self.vault.write(&self.path, &json)?)
    }
}

impl SecretStore for VaultSecretStore {
    fn get(&self, secret: &SecretRef) -> Result<Option<Zeroizing<String>>> {
        let mut secrets = self.read()?;
        Ok(secrets.0.remove(&secret.account()).map(Zeroizing::new))
    }

    fn set(&self, secret: &SecretRef, value: &str) -> Result<()> {
        self.change(|secrets| {
            if let Some(mut old) = secrets.insert(secret.account(), value.to_owned()) {
                old.zeroize();
            }
        })
    }

    fn delete(&self, secret: &SecretRef) -> Result<()> {
        self.change(|secrets| {
            if let Some(mut old) = secrets.remove(&secret.account()) {
                old.zeroize();
            }
        })
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use shellrs_hosts::secrets::{SecretRef, SecretStore as _};

    use super::VaultSecretStore;
    use crate::vault::{Vault, tests::QUICK};

    #[test]
    fn secrets_are_sealed_and_out_of_reach_while_locked() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::with_cost(dir.path(), QUICK));
        vault.create("first password").unwrap();
        let store = VaultSecretStore::new(vault.clone());
        let (one, two) = (
            SecretRef::password("root", "10.0.1.12", 22),
            SecretRef::credential("deploy"),
        );
        assert!(store.get(&one).unwrap().is_none());
        store.set(&one, "hunter2").unwrap();
        store.set(&two, "correct horse").unwrap();
        store.set(&one, "hunter3").unwrap();
        assert_eq!(store.get(&one).unwrap().unwrap().as_str(), "hunter3");
        store.delete(&two).unwrap();
        store.delete(&two).unwrap();
        assert!(store.get(&two).unwrap().is_none());
        let file = std::fs::read(dir.path().join("secrets")).unwrap();
        assert!(!file.windows(7).any(|part| part == b"hunter3"));

        vault.lock();
        assert!(store.get(&one).is_err());
        assert!(store.set(&one, "other").is_err());
        vault.unlock("first password").unwrap();
        assert_eq!(store.get(&one).unwrap().unwrap().as_str(), "hunter3");
    }
}
