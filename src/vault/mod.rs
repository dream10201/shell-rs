//! Everything ShellRS keeps on this computer, sealed with a key that only
//! the password opens: XChaCha20-Poly1305 over each file, under a random
//! data key that an Argon2id key of the password wraps.
//!
//! The header beside the files (`vault.json`) holds what the password needs
//! to become that key: the cost, the salt and the wrapped data key. Nothing
//! in it or in memory after [`Vault::lock`] gets to the data without the
//! password.

mod key;
mod secrets;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead as _, KeyInit as _, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::Rng as _;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use key::Key;
pub use secrets::VaultSecretStore;

/// What Argon2id is asked to spend on one guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cost {
    pub memory_kib: u32,
    pub iterations: u32,
    pub lanes: u32,
}

impl Cost {
    /// New vaults: four times the memory of RFC 9106's second
    /// recommendation.
    pub const DEFAULT: Cost = Cost {
        memory_kib: 256 * 1024,
        iterations: 3,
        lanes: 4,
    };
    /// A header asking for more is not one ShellRS wrote.
    const MOST: Cost = Cost {
        memory_kib: 4 * 1024 * 1024,
        iterations: 64,
        lanes: 64,
    };
}

const VERSION: u8 = 1;
const HEADER: &str = "vault.json";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
/// What every sealed file starts with.
const MAGIC: &[u8; 5] = b"SRSV\x01";
/// What the wrapped data key is bound to.
const WRAP: &[u8] = b"shellrs vault key";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    version: u8,
    cost: Cost,
    salt: Vec<u8>,
    nonce: Vec<u8>,
    key: Vec<u8>,
}

#[derive(Debug)]
pub enum VaultError {
    /// The key is gone until the password is given again.
    Locked,
    WrongPassword,
    /// There is a vault already, or none yet.
    Unexpected,
    /// The header or a file is not what ShellRS wrote.
    Damaged,
    Io(io::Error),
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultError::Locked => f.write_str("locked"),
            VaultError::WrongPassword => f.write_str("wrong password"),
            VaultError::Unexpected => f.write_str("unexpected vault state"),
            VaultError::Damaged => f.write_str("damaged"),
            VaultError::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for VaultError {}

impl From<io::Error> for VaultError {
    fn from(error: io::Error) -> Self {
        VaultError::Io(error)
    }
}

impl From<VaultError> for io::Error {
    fn from(error: VaultError) -> Self {
        match error {
            VaultError::Io(error) => error,
            VaultError::Locked => io::Error::new(io::ErrorKind::PermissionDenied, error),
            other => io::Error::new(io::ErrorKind::InvalidData, other),
        }
    }
}

pub struct Vault {
    dir: PathBuf,
    cost: Cost,
    key: Mutex<Option<Key>>,
}

impl Vault {
    /// The vault whose files are in `dir`; locked, and perhaps not made yet.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self::with_cost(dir, Cost::DEFAULT)
    }

    /// With what a new password costs to try: tests ask for little.
    pub fn with_cost(dir: impl Into<PathBuf>, cost: Cost) -> Self {
        Self {
            dir: dir.into(),
            cost,
            key: Mutex::new(None),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether a password has been set here. A folder that holds anything
    /// counts: sealed files whose header is gone must not get a new key
    /// made over them.
    pub fn exists(&self) -> bool {
        // Not what a system or an interrupted write leaves behind.
        std::fs::read_dir(&self.dir).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                !name.starts_with('.') && !name.ends_with(".tmp")
            })
        })
    }

    pub fn is_unlocked(&self) -> bool {
        self.key().is_some()
    }

    /// Forget the key. What is sealed stays so until the password comes
    /// again.
    pub fn lock(&self) {
        *self.key() = None;
    }

    /// Make the vault: a new data key under `password`. Blocks for the
    /// time Argon2id takes.
    pub fn create(&self, password: &str) -> Result<(), VaultError> {
        if self.exists() {
            return Err(VaultError::Unexpected);
        }
        let mut data_key = Key::zeroed();
        rand::rng().fill_bytes(data_key.bytes_mut());
        self.write_header(password, &data_key)?;
        *self.key() = Some(data_key);
        Ok(())
    }

    /// Open the vault with its password. Blocks for the time Argon2id takes.
    pub fn unlock(&self, password: &str) -> Result<(), VaultError> {
        let data_key = self.unwrap_key(password)?;
        *self.key() = Some(data_key);
        Ok(())
    }

    /// Put the data key under another password. The files stay as they are.
    pub fn change_password(&self, current: &str, new: &str) -> Result<(), VaultError> {
        let data_key = self.unwrap_key(current)?;
        self.write_header(new, &data_key)
    }

    fn key(&self) -> MutexGuard<'_, Option<Key>> {
        self.key.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn header(&self) -> Result<Header, VaultError> {
        let bytes = match std::fs::read(self.dir.join(HEADER)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(if self.exists() {
                    VaultError::Damaged
                } else {
                    VaultError::Unexpected
                });
            }
            Err(error) => return Err(error.into()),
        };
        let header: Header = serde_json::from_slice(&bytes).map_err(|_| VaultError::Damaged)?;
        let cost = header.cost;
        let sound = header.version == VERSION
            && header.nonce.len() == NONCE_LEN
            && header.salt.len() >= 16
            && cost.memory_kib <= Cost::MOST.memory_kib
            && cost.iterations <= Cost::MOST.iterations
            && cost.lanes <= Cost::MOST.lanes;
        if sound {
            Ok(header)
        } else {
            Err(VaultError::Damaged)
        }
    }

    fn unwrap_key(&self, password: &str) -> Result<Key, VaultError> {
        let header = self.header()?;
        let wrapping = derive(password, &header.salt, header.cost)?;
        let nonce: [u8; NONCE_LEN] = header.nonce[..]
            .try_into()
            .map_err(|_| VaultError::Damaged)?;
        let opened = Zeroizing::new(
            open(wrapping.bytes(), &nonce, &header.key, WRAP)
                .map_err(|_| VaultError::WrongPassword)?,
        );
        let mut data_key = Key::zeroed();
        if opened.len() != KEY_LEN {
            return Err(VaultError::Damaged);
        }
        data_key.bytes_mut().copy_from_slice(&opened);
        Ok(data_key)
    }

    fn write_header(&self, password: &str, data_key: &Key) -> Result<(), VaultError> {
        let mut salt = vec![0; 32];
        rand::rng().fill_bytes(&mut salt);
        let wrapping = derive(password, &salt, self.cost)?;
        let (nonce, key) = seal(wrapping.bytes(), data_key.bytes(), WRAP)?;
        let header = Header {
            version: VERSION,
            cost: self.cost,
            salt,
            nonce: nonce.to_vec(),
            key,
        };
        let json = serde_json::to_vec_pretty(&header).map_err(io::Error::other)?;
        shellrs_ssh::files::replace(&self.dir.join(HEADER), &json)?;
        Ok(())
    }

    /// What a sealed file is bound to: its place in the vault, so that one
    /// file cannot be put in another's.
    fn name(&self, path: &Path) -> Option<String> {
        let inside = path.strip_prefix(&self.dir).ok()?;
        Some(
            inside
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }

    /// Seal `bytes` as the file `name`.
    fn seal_file(&self, name: &str, bytes: &[u8]) -> Result<Vec<u8>, VaultError> {
        let key = self.key();
        let key = key.as_ref().ok_or(VaultError::Locked)?;
        let (nonce, sealed) = seal(key.bytes(), bytes, name.as_bytes())?;
        let mut file = Vec::with_capacity(MAGIC.len() + NONCE_LEN + sealed.len());
        file.extend_from_slice(MAGIC);
        file.extend_from_slice(&nonce);
        file.extend_from_slice(&sealed);
        Ok(file)
    }

    fn open_file(&self, name: &str, file: &[u8]) -> Result<Vec<u8>, VaultError> {
        let key = self.key();
        let key = key.as_ref().ok_or(VaultError::Locked)?;
        let rest = file.strip_prefix(MAGIC).ok_or(VaultError::Damaged)?;
        let (nonce, sealed) = rest
            .split_first_chunk::<NONCE_LEN>()
            .ok_or(VaultError::Damaged)?;
        open(key.bytes(), nonce, sealed, name.as_bytes()).map_err(|_| VaultError::Damaged)
    }
}

/// Files in the vault's folder are sealed; any other, such as a private
/// key of the user's own, is read and written as it is.
impl shellrs_ssh::files::Files for Vault {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        let file = std::fs::read(path)?;
        match self.name(path) {
            Some(name) => Ok(self.open_file(&name, &file)?),
            None => Ok(file),
        }
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        match self.name(path) {
            Some(name) => shellrs_ssh::files::replace(path, &self.seal_file(&name, bytes)?),
            None => shellrs_ssh::files::replace(path, bytes),
        }
    }
}

fn derive(password: &str, salt: &[u8], cost: Cost) -> Result<Key, VaultError> {
    let params = Params::new(cost.memory_kib, cost.iterations, cost.lanes, Some(KEY_LEN))
        .map_err(|_| VaultError::Damaged)?;
    let mut key = Key::zeroed();
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.bytes_mut())
        .map_err(|error| VaultError::Io(io::Error::other(error.to_string())))?;
    Ok(key)
}

fn seal(
    key: &[u8; KEY_LEN],
    bytes: &[u8],
    bound_to: &[u8],
) -> Result<([u8; NONCE_LEN], Vec<u8>), VaultError> {
    let mut nonce = [0; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let sealed = XChaCha20Poly1305::new(key.into())
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: bytes,
                aad: bound_to,
            },
        )
        .map_err(|_| VaultError::Damaged)?;
    Ok((nonce, sealed))
}

fn open(
    key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    sealed: &[u8],
    bound_to: &[u8],
) -> Result<Vec<u8>, chacha20poly1305::aead::Error> {
    XChaCha20Poly1305::new(key.into()).decrypt(
        &XNonce::from(*nonce),
        Payload {
            msg: sealed,
            aad: bound_to,
        },
    )
}

/// Whether a file could not be read or written only because the vault is
/// locked: it can be once the password is given.
pub fn is_locked(error: &io::Error) -> bool {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<VaultError>())
        .is_some_and(|error| matches!(error, VaultError::Locked))
}

static VAULT: OnceLock<Arc<Vault>> = OnceLock::new();

/// Seal ShellRS's files in `vault` from now on. Once, at startup.
pub fn install(vault: Arc<Vault>) {
    shellrs_ssh::files::install(vault.clone());
    let _ = VAULT.set(vault);
}

/// The vault ShellRS runs on; none in tests, whose files stay plain.
pub fn installed() -> Option<&'static Arc<Vault>> {
    VAULT.get()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Cost, Vault, VaultError};
    use shellrs_ssh::files::Files as _;

    /// Cheap enough to try in a test.
    pub(crate) const QUICK: Cost = Cost {
        memory_kib: 64,
        iterations: 1,
        lanes: 1,
    };

    #[test]
    fn a_vault_opens_only_with_its_password_and_forgets_its_key_when_locked() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::with_cost(dir.path(), QUICK);
        assert!(!vault.exists() && !vault.is_unlocked());
        assert!(matches!(vault.unlock("x"), Err(VaultError::Unexpected)));
        vault.create("first password").unwrap();
        assert!(vault.exists() && vault.is_unlocked());
        assert!(matches!(vault.create("again"), Err(VaultError::Unexpected)));
        let header = std::fs::read_to_string(dir.path().join("vault.json")).unwrap();
        assert!(!header.contains("first password"));

        let file = dir.path().join("keys").join("one");
        vault.write(&file, b"secret text").unwrap();
        let sealed = std::fs::read(&file).unwrap();
        assert!(!sealed.windows(6).any(|part| part == b"secret"));
        assert_eq!(vault.read(&file).unwrap(), b"secret text");

        vault.lock();
        assert!(!vault.is_unlocked());
        assert!(super::is_locked(&vault.read(&file).unwrap_err()));
        // A folder that really refuses is another matter.
        assert!(!super::is_locked(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )));
        assert!(vault.write(&file, b"other").is_err());
        assert_eq!(std::fs::read(&file).unwrap(), sealed);

        let again = Vault::with_cost(dir.path(), QUICK);
        assert!(matches!(
            again.unlock("wrong password"),
            Err(VaultError::WrongPassword)
        ));
        assert!(!again.is_unlocked());
        again.unlock("first password").unwrap();
        assert_eq!(again.read(&file).unwrap(), b"secret text");
    }

    #[test]
    fn a_new_password_opens_the_same_files() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::with_cost(dir.path(), QUICK);
        vault.create("first password").unwrap();
        let file = dir.path().join("settings");
        vault.write(&file, b"{}").unwrap();
        assert!(matches!(
            vault.change_password("wrong", "second password"),
            Err(VaultError::WrongPassword)
        ));
        vault
            .change_password("first password", "second password")
            .unwrap();
        let again = Vault::with_cost(dir.path(), QUICK);
        assert!(again.unlock("first password").is_err());
        again.unlock("second password").unwrap();
        assert_eq!(again.read(&file).unwrap(), b"{}");
    }

    #[test]
    fn a_file_moved_changed_or_left_plain_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::with_cost(dir.path(), QUICK);
        vault.create("first password").unwrap();
        let (one, two) = (dir.path().join("one"), dir.path().join("two"));
        vault.write(&one, b"one").unwrap();
        std::fs::copy(&one, &two).unwrap();
        assert!(vault.read(&two).is_err());
        let mut bytes = std::fs::read(&one).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        std::fs::write(&one, bytes).unwrap();
        assert!(vault.read(&one).is_err());
        std::fs::write(&one, b"plain").unwrap();
        assert!(vault.read(&one).is_err());

        // A file of the user's own, outside the vault, is read as it is.
        let outside = tempfile::tempdir().unwrap();
        let theirs = outside.path().join("id_ed25519");
        std::fs::write(&theirs, b"their key").unwrap();
        assert_eq!(vault.read(&theirs).unwrap(), b"their key");
    }

    #[test]
    fn a_damaged_header_opens_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::with_cost(dir.path(), QUICK);
        vault.create("first password").unwrap();
        vault.lock();
        let path = dir.path().join("vault.json");
        let header = std::fs::read_to_string(&path).unwrap();
        // Without its header a vault with files in it is damaged, not new.
        vault
            .write(&dir.path().join("settings"), b"{}")
            .unwrap_err();
        std::fs::write(dir.path().join("settings"), b"sealed").unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(vault.exists());
        assert!(matches!(
            vault.create("another"),
            Err(VaultError::Unexpected)
        ));
        assert!(matches!(
            vault.unlock("first password"),
            Err(VaultError::Damaged)
        ));
        for broken in [
            "not json".to_string(),
            header.replace("\"version\": 1", "\"version\": 2"),
            header.replace("\"memory_kib\": 64", "\"memory_kib\": 4294967295"),
        ] {
            std::fs::write(&path, broken).unwrap();
            assert!(matches!(
                vault.unlock("first password"),
                Err(VaultError::Damaged)
            ));
        }
        // Another cost derives another key.
        std::fs::write(
            &path,
            header.replace("\"iterations\": 1", "\"iterations\": 2"),
        )
        .unwrap();
        assert!(matches!(
            vault.unlock("first password"),
            Err(VaultError::WrongPassword)
        ));
    }
}
