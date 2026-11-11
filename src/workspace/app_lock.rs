use super::Workspace;
use crate::app::{ChangeAppLockPassword, DisableAppLock, EnableAppLock, LockApp};
use crate::app_lock::{AppLock, PasswordOperation, open_password_dialog};
use gpui_kit::*;
use std::time::Duration;

impl Workspace {
    fn configure_app_lock(
        &self,
        operation: PasswordOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(lock) = AppLock::global(cx) {
            open_password_dialog(lock, operation, window, cx);
        }
    }
    pub(super) fn on_enable_app_lock(
        &mut self,
        _: &EnableAppLock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.configure_app_lock(PasswordOperation::Enable, window, cx);
    }
    pub(super) fn on_change_app_lock_password(
        &mut self,
        _: &ChangeAppLockPassword,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.configure_app_lock(PasswordOperation::Change, window, cx);
    }
    pub(super) fn on_disable_app_lock(
        &mut self,
        _: &DisableAppLock,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.configure_app_lock(PasswordOperation::Disable, window, cx);
    }
    pub(super) fn on_lock_app(&mut self, _: &LockApp, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(lock) = AppLock::global(cx) {
            self.lock_app(&lock, cx);
        }
    }

    fn lock_app(&mut self, lock: &Entity<AppLock>, cx: &mut Context<Self>) {
        if !lock.read(cx).is_enabled() || lock.read(cx).is_busy() {
            return;
        }
        lock.update(cx, |lock, cx| lock.lock(cx));
    }

    pub(super) fn watch_idle_lock(
        lock: Entity<AppLock>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this
                    .update(cx, |this, cx| {
                        let settings = this.settings().read(cx).settings();
                        let idle =
                            Duration::from_secs(u64::from(settings.app_lock_idle_minutes) * 60);
                        let with_screen = settings.app_lock_with_screen;
                        let lock_state = lock.read(cx);
                        if lock_state.is_idle(idle, cx)
                            || (with_screen && lock_state.screen_locked())
                        {
                            this.lock_app(&lock, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
}
