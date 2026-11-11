//! A key in memory: kept out of the swap file where the system allows,
//! and wiped when dropped.

use zeroize::Zeroize as _;

const LEN: usize = 32;

pub(super) struct Key(Box<[u8; LEN]>);

impl Key {
    pub(super) fn zeroed() -> Self {
        let key = Self(Box::new([0; LEN]));
        pin(key.0.as_ptr());
        key
    }

    pub(super) fn bytes(&self) -> &[u8; LEN] {
        &self.0
    }

    pub(super) fn bytes_mut(&mut self) -> &mut [u8; LEN] {
        &mut self.0
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Keep the key's page in memory. Best effort: a system that refuses
/// leaves the key where it is. The page is never let go again: unpinning
/// one key would unpin every other key on its page.
#[cfg(unix)]
fn pin(key: *const u8) {
    // SAFETY: `key` points at `LEN` bytes this module owns.
    unsafe {
        libc::mlock(key.cast(), LEN);
    }
}

#[cfg(windows)]
fn pin(key: *const u8) {
    // SAFETY: `key` points at `LEN` bytes this module owns.
    unsafe {
        windows_sys::Win32::System::Memory::VirtualLock(key.cast(), LEN);
    }
}

#[cfg(not(any(unix, windows)))]
fn pin(_: *const u8) {}
