//! The application lock: the password that opens the vault everything is
//! sealed in. One process-wide entity gates the UI; locking forgets the
//! vault's key, so nothing sealed can be read until the password comes
//! again.
mod activity;
mod password_form;
mod screen;
pub use screen::{NoScreen, ScreenState, system_screen};

pub use activity::activity_tracker;
use gpui_kit::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

use crate::i18n::t;
use crate::vault::{Vault, VaultError};
pub use password_form::{UnlockScreen, open_password_dialog};

/// The shortest password a vault is made with.
pub const SHORTEST_PASSWORD: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordOperation {
    /// The first start: choose the password.
    Create,
    Change,
    Unlock,
}

fn failure(error: VaultError) -> SharedString {
    match error {
        VaultError::WrongPassword => t!("app_lock.wrong_password"),
        VaultError::Damaged => t!("app_lock.damaged"),
        VaultError::Locked | VaultError::Unexpected => t!("app_lock.unavailable"),
        VaultError::Io(error) => t!("app_lock.failed", error = error),
    }
}

pub struct AppLock {
    vault: Arc<Vault>,
    locked: bool,
    busy: bool,
    last_activity: Option<Instant>,
    screen: Arc<dyn ScreenState>,
    _keyboard_activity: Option<Subscription>,
}

#[derive(Clone)]
struct LockGlobal(Entity<AppLock>);
impl Global for LockGlobal {}

impl AppLock {
    /// Locked until the vault's password is given, or chosen on a first
    /// start. No data is read before that.
    pub fn new(vault: Arc<Vault>) -> Self {
        Self {
            locked: !vault.is_unlocked(),
            vault,
            busy: false,
            last_activity: None,
            screen: Arc::new(NoScreen),
            _keyboard_activity: None,
        }
    }

    pub fn install(lock: Entity<Self>, cx: &mut App) {
        let weak = lock.downgrade();
        let subscription = cx.intercept_keystrokes(move |_, _, cx| {
            let _ = weak.update(cx, |lock, cx| lock.record_activity(cx));
        });
        lock.update(cx, |lock, cx| {
            lock._keyboard_activity = Some(subscription);
            lock.record_activity(cx);
        });
        cx.set_global(LockGlobal(lock));
    }
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<LockGlobal>().map(|g| g.0.clone())
    }
    pub fn is_locked(&self) -> bool {
        self.locked
    }
    /// Whether this is a first start: there is no password yet.
    pub fn needs_password(&self) -> bool {
        self.locked && !self.vault.exists()
    }
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// Input does not notify observers: activity must not redraw the app.
    pub fn record_activity(&mut self, cx: &App) {
        if !self.locked {
            self.last_activity = Some(cx.background_executor().now());
        }
    }

    pub fn is_idle(&self, timeout: Duration, cx: &App) -> bool {
        !timeout.is_zero()
            && !self.locked
            && !self.busy
            && self.last_activity.is_some_and(|last| {
                cx.background_executor()
                    .now()
                    .saturating_duration_since(last)
                    >= timeout
            })
    }

    /// Where 电脑锁屏时锁定 learns of the computer's lock screen.
    pub fn watch_screen(&mut self, screen: Arc<dyn ScreenState>) {
        self.screen = screen;
    }

    /// Whether the computer's lock screen is showing over an open ShellRS.
    pub fn screen_locked(&self) -> bool {
        !self.locked && !self.busy && self.screen.is_locked()
    }

    /// Lock, and forget the vault's key. The caller has written what was
    /// still to be written: after this nothing can be sealed.
    pub fn lock(&mut self, cx: &mut Context<Self>) {
        if !self.locked && !self.busy {
            self.vault.lock();
            self.locked = true;
            cx.notify();
        }
    }

    fn submit(
        &mut self,
        operation: PasswordOperation,
        current: Zeroizing<String>,
        new: Zeroizing<String>,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), SharedString>> {
        let expected = match operation {
            PasswordOperation::Create => self.needs_password(),
            PasswordOperation::Unlock => self.locked && self.vault.exists(),
            PasswordOperation::Change => !self.locked,
        };
        if self.busy || !expected {
            return Task::ready(Err(t!("app_lock.unavailable")));
        }
        if operation != PasswordOperation::Unlock && new.chars().count() < SHORTEST_PASSWORD {
            return Task::ready(Err(t!("app_lock.too_short")));
        }
        self.busy = true;
        cx.notify();
        let vault = self.vault.clone();
        // Keep the service alive even if its dialog is dismissed during work.
        let service = cx.entity();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_spawn(async move {
                    match operation {
                        PasswordOperation::Create => vault.create(&new),
                        PasswordOperation::Unlock => vault.unlock(&current),
                        PasswordOperation::Change => vault.change_password(&current, &new),
                    }
                })
                .await;
            service.update(cx, |this, cx| {
                this.busy = false;
                let outcome = result.map_err(failure).map(|()| {
                    this.locked = false;
                    this.record_activity(cx);
                });
                cx.notify();
                outcome
            })
        })
    }
}

/// Tests without an installed gate are unlocked.
pub fn is_locked(cx: &App) -> bool {
    AppLock::global(cx).is_some_and(|lock| lock.read(cx).is_locked())
}

#[cfg(test)]
mod tests {
    use super::{AppLock, PasswordOperation, ScreenState};
    use crate::vault::{Vault, tests::QUICK};
    use gpui_kit::{AppContext as _, Entity, TestAppContext};
    use std::sync::{Arc, atomic::Ordering};
    use zeroize::Zeroizing;

    fn open(cx: &mut TestAppContext, dir: &std::path::Path) -> (Entity<AppLock>, Arc<Vault>) {
        let vault = Arc::new(Vault::with_cost(dir, QUICK));
        let lock = cx.update(|cx| cx.new(|_| AppLock::new(vault.clone())));
        (lock, vault)
    }

    fn submit(
        cx: &mut TestAppContext,
        lock: &Entity<AppLock>,
        operation: PasswordOperation,
        current: &str,
        new: &str,
    ) -> bool {
        let task = cx.update(|cx| {
            lock.update(cx, |lock, cx| {
                lock.submit(
                    operation,
                    Zeroizing::new(current.into()),
                    Zeroizing::new(new.into()),
                    cx,
                )
            })
        });
        cx.foreground_executor.block_test(task).is_ok()
    }

    #[gpui_kit::test]
    fn the_password_is_chosen_once_then_asked_for_and_can_be_changed(cx: &mut TestAppContext) {
        use PasswordOperation::{Change, Create, Unlock};
        let dir = tempfile::tempdir().unwrap();
        let (lock, vault) = open(cx, dir.path());
        cx.update(|cx| assert!(lock.read(cx).is_locked() && lock.read(cx).needs_password()));
        assert!(!submit(cx, &lock, Unlock, "first password", ""));
        assert!(!submit(cx, &lock, Create, "", "short"));
        assert!(submit(cx, &lock, Create, "", "first password"));
        cx.update(|cx| assert!(!lock.read(cx).is_locked()));
        assert!(vault.is_unlocked());
        assert!(!submit(cx, &lock, Create, "", "another password"));

        // Locking forgets the key; only the password brings it back.
        cx.update(|cx| lock.update(cx, |lock, cx| lock.lock(cx)));
        assert!(!vault.is_unlocked());
        cx.update(|cx| assert!(lock.read(cx).is_locked() && !lock.read(cx).needs_password()));
        assert!(!submit(
            cx,
            &lock,
            Change,
            "first password",
            "second password"
        ));
        assert!(!submit(cx, &lock, Unlock, "incorrect", ""));
        assert!(!vault.is_unlocked());
        assert!(submit(cx, &lock, Unlock, "first password", ""));
        assert!(vault.is_unlocked());

        assert!(!submit(cx, &lock, Change, "incorrect", "second password"));
        assert!(!submit(cx, &lock, Change, "first password", "short"));
        assert!(submit(
            cx,
            &lock,
            Change,
            "first password",
            "second password"
        ));

        // A restart asks for the new one.
        let (lock, vault) = open(cx, dir.path());
        cx.update(|cx| assert!(lock.read(cx).is_locked() && !lock.read(cx).needs_password()));
        assert!(!submit(cx, &lock, Unlock, "first password", ""));
        assert!(submit(cx, &lock, Unlock, "second password", ""));
        assert!(vault.is_unlocked());
    }

    struct Screen(std::sync::atomic::AtomicBool);
    impl ScreenState for Screen {
        fn is_locked(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    #[gpui_kit::test]
    fn the_computers_lock_screen_counts_only_over_an_open_lock(cx: &mut TestAppContext) {
        let screen = Arc::new(Screen(true.into()));
        let dir = tempfile::tempdir().unwrap();
        let (lock, _) = open(cx, dir.path());
        cx.update(|cx| {
            lock.update(cx, |lock, _| lock.watch_screen(screen.clone()));
            assert!(!lock.read(cx).screen_locked());
        });
        assert!(submit(
            cx,
            &lock,
            PasswordOperation::Create,
            "",
            "first password"
        ));
        cx.update(|cx| {
            assert!(lock.read(cx).screen_locked());
            lock.update(cx, |lock, cx| lock.lock(cx));
            assert!(!lock.read(cx).screen_locked());
        });
    }
}
