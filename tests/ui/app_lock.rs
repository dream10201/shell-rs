//! Exercise the real startup shell, settings and password gate with a vault
//! in a temporary folder and injected workspace transports.
use crate::support::*;
use gpui_kit::AnyView;
use shellrs::app_lock::AppLock;
use shellrs::vault::{Cost, Vault};
use shellrs::workspace::AppShell;
use std::cell::Cell;
use std::rc::Rc;

/// Cheap enough to try in a test.
const QUICK: Cost = Cost {
    memory_kib: 64,
    iterations: 1,
    lanes: 1,
};

fn open_locked_workspace(
    cx: &mut TestAppContext,
    dir: &std::path::Path,
) -> (WindowHandle<Root>, Entity<AppLock>, Rc<Cell<usize>>) {
    let (handle, lock, created, _) = open_locked_workspace_with_settings(cx, dir, settings_store());
    (handle, lock, created)
}

/// A first start: choose the password on the screen that asks for one.
fn choose_password(
    cx: &mut TestAppContext,
    handle: WindowHandle<Root>,
    lock: &Entity<AppLock>,
    password: &str,
) {
    in_frame(cx, handle, |window, cx| {
        enter(window, "app-lock-new-password", password, cx);
        enter(window, "app-lock-repeat-password", password, cx);
        window.click("app-lock-unlock", cx);
    });
    wait_for_password(cx, lock);
}

fn open_locked_workspace_with_settings(
    cx: &mut TestAppContext,
    dir: &std::path::Path,
    settings: SettingsStore,
) -> (
    WindowHandle<Root>,
    Entity<AppLock>,
    Rc<Cell<usize>>,
    Entity<SettingsStore>,
) {
    if cx.update(|cx| AppLock::global(cx).is_none()) {
        init_app(cx);
    }
    cx.update(|cx| cx.set_reduce_motion(true));
    let lock = cx.update(|cx| {
        let lock = cx.new(|_| AppLock::new(Arc::new(Vault::with_cost(dir, QUICK))));
        AppLock::install(lock.clone(), cx);
        lock
    });
    let settings = cx.update(|cx| cx.new(|_| settings));
    let initial_settings = settings.clone();
    let created = Rc::new(Cell::new(0));
    let count = created.clone();
    let gate = lock.clone();
    let handle = cx.open_window(size(px(1100.), px(760.)), move |window, cx| {
        let shell = cx.new(|cx| {
            AppShell::new(
                gate,
                None,
                move |_, _, window, cx| {
                    count.set(count.get() + 1);
                    let store = cx.new(|_| HostStore::empty());
                    let settings = initial_settings;
                    let workspace = cx.new(|cx| {
                        Workspace::new_with_services(
                            store,
                            settings,
                            Arc::new(FixedRemoteTerminalTransportProvider::new(Arc::new(
                                FakeTerminalFactory::default(),
                            ))),
                            fake_telnet_terminals(),
                            fake_serial_terminals(),
                            fake_local_terminals(),
                            Arc::new(FakeConnectionTester::default()),
                            Arc::new(FakeForwardProvider::default()),
                            window,
                            cx,
                        )
                    });
                    AnyView::from(workspace)
                },
                window,
                cx,
            )
        });
        Root::new(shell, window, cx)
    });
    cx.run_until_parked();
    (handle, lock, created, settings)
}

fn app_settings(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    in_frame(cx, handle, |window, cx| {
        window.dispatch_action(Box::new(OpenSettings), cx)
    });
    in_frame(cx, handle, |window, cx| {
        window.within("settings").click("0-5", cx)
    });
}

fn enter(window: &mut gpui_kit::Window, id: &'static str, text: &str, cx: &mut App) {
    window.click(id, cx);
    window.input(text, cx);
}

fn wait_for_password(cx: &mut TestAppContext, lock: &Entity<AppLock>) {
    // Real Argon2 runs on the background executor. Drain until its UI result
    // has been handled, without real servers, timers or the system keychain.
    cx.run_until_parked();
    cx.update(|cx| assert!(!lock.read(cx).is_busy()));
}

#[gpui_kit::test]
fn the_password_is_chosen_at_the_first_start_and_asked_for_from_then_on(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, lock, created) = open_locked_workspace(cx, dir.path());
    // Nothing is made before there is a password.
    assert_eq!(created.get(), 0);
    in_frame(cx, handle, |window, cx| {
        assert!(window.find("app-lock-screen").visible());
        assert!(window.try_find("app-lock-password").is_none());
        assert_eq!(window.find("app-lock-new-password").focused(), Some(true));
        enter(window, "app-lock-new-password", "first password", cx);
        enter(window, "app-lock-repeat-password", "different password", cx);
        window.click("app-lock-unlock", cx);
    });
    in_frame(cx, handle, |window, _| {
        assert_eq!(
            window.find("app-lock-error").label(),
            Some("两次输入的密码不一致。")
        );
    });
    assert!(!dir.path().join("vault.json").exists());
    in_frame(cx, handle, |window, cx| {
        window.click("app-lock-repeat-password", cx);
        window.press("secondary-a", cx);
        window.input("first password", cx);
        window.press("enter", cx);
    });
    wait_for_password(cx, &lock);
    assert_eq!(created.get(), 1);
    assert!(dir.path().join("vault.json").exists());

    app_settings(cx, handle);
    in_frame(cx, handle, |window, cx| window.click("app-lock-now", cx));
    in_frame(cx, handle, |window, cx| {
        assert!(window.find("app-lock-screen").visible());
        assert!(window.try_find("settings").is_none());
        // Locked, it asks for the password, not for a new one.
        assert!(window.try_find("app-lock-new-password").is_none());
        window.dispatch_action(Box::new(OpenSettings), cx);
        window.press("escape", cx);
        assert!(window.find("app-lock-screen").visible());
        enter(window, "app-lock-password", "incorrect", cx);
        window.press("enter", cx);
    });
    wait_for_password(cx, &lock);
    in_frame(cx, handle, |window, cx| {
        assert!(window.find("app-lock-error").visible());
        assert!(window.try_find("settings").is_none());
        enter(window, "app-lock-password", "first password", cx);
        window.press("enter", cx);
    });
    wait_for_password(cx, &lock);
    in_frame(cx, handle, |window, _| {
        assert!(window.find("settings").visible())
    });
    assert_eq!(created.get(), 1, "manual unlock preserves the workspace");

    in_frame(cx, handle, |window, cx| window.click("app-lock-change", cx));
    in_frame(cx, handle, |window, cx| {
        enter(window, "app-lock-password", "first password", cx);
        enter(window, "app-lock-new-password", "second password", cx);
        enter(window, "app-lock-repeat-password", "second password", cx);
        assert_eq!(
            window.find("app-lock-new-password").bounds().left(),
            window.find("app-lock-repeat-password").bounds().left()
        );
        window.click("app-lock-commit", cx);
    });
    wait_for_password(cx, &lock);

    // A fresh launch has no workspace until the password is accepted.
    let (reopened, fresh_lock, fresh_created) = open_locked_workspace(cx, dir.path());
    cx.update(|_| shellrs::i18n::set_locale("en"));
    assert_eq!(fresh_created.get(), 0);
    in_frame(cx, reopened, |window, cx| {
        assert!(window.find("app-lock-screen").visible());
        assert!(window.try_find("host-search").is_none());
        assert_eq!(window.find("app-lock-password").focused(), Some(true));
        enter(window, "app-lock-password", "first password", cx);
        window.click("app-lock-unlock", cx);
    });
    wait_for_password(cx, &fresh_lock);
    assert_eq!(fresh_created.get(), 0);
    in_frame(cx, reopened, |window, cx| {
        assert_eq!(
            window.find("app-lock-error").label(),
            Some("The app lock password is incorrect. Try again.")
        );
        enter(window, "app-lock-password", "second password", cx);
        window.click("app-lock-unlock", cx);
    });
    wait_for_password(cx, &fresh_lock);
    assert_eq!(fresh_created.get(), 1);
}

#[gpui_kit::test]
fn a_damaged_key_file_never_opens_the_workspace(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("vault.json"), "broken").unwrap();
    let (handle, lock, created) = open_locked_workspace(cx, dir.path());
    in_frame(cx, handle, |window, cx| {
        // There is a vault: no new password is offered over it.
        assert!(window.try_find("app-lock-new-password").is_none());
        enter(window, "app-lock-password", "any password", cx);
        window.click("app-lock-unlock", cx);
    });
    wait_for_password(cx, &lock);
    in_frame(cx, handle, |window, cx| {
        assert!(window.find("app-lock-error").visible());
        window.press("escape", cx);
        window.dispatch_action(Box::new(OpenSettings), cx);
        assert!(window.find("app-lock-screen").visible());
    });
    assert_eq!(created.get(), 0);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("vault.json")).unwrap(),
        "broken"
    );
}

#[gpui_kit::test]
fn app_lock_idle_timeout_tracks_input_and_restarts_after_unlock(cx: &mut TestAppContext) {
    use gpui_kit::ScrollDelta;
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let (handle, lock, _) = open_locked_workspace(cx, dir.path());
    choose_password(cx, handle, &lock, "idle password");
    app_settings(cx, handle);
    let advance = |seconds, cx: &mut TestAppContext| {
        cx.executor().advance_clock(Duration::from_secs(seconds));
        cx.run_until_parked();
    };
    let locked = |cx: &mut TestAppContext| cx.update(|cx| lock.read(cx).is_locked());
    // The default is zero: even a long idle period does not lock.
    advance(120, cx);
    assert!(!locked(cx));
    in_frame(cx, handle, |window, cx| {
        window
            .within("settings")
            .within("group-1")
            .within("item-1")
            .click("increment", cx);
    });
    advance(50, cx);
    assert!(!locked(cx));
    in_frame(cx, handle, |window, cx| window.press("tab", cx));
    advance(50, cx);
    assert!(!locked(cx), "keyboard input resets the timeout");
    in_frame(cx, handle, |window, cx| window.hover("app-lock-now", cx));
    advance(50, cx);
    assert!(!locked(cx), "pointer movement resets the timeout");
    in_frame(cx, handle, |window, cx| {
        window.scroll(
            "app-lock-now",
            ScrollDelta::Pixels(point(px(0.), px(-10.))),
            cx,
        )
    });
    advance(50, cx);
    assert!(!locked(cx), "scrolling resets the timeout");
    in_frame(cx, handle, |window, cx| window.click("app-lock-change", cx));
    advance(50, cx);
    // A rendered frame or a notification is not user input.
    in_frame(cx, handle, |window, cx| {
        window.push_notification(Notification::info("background update"), cx)
    });
    advance(11, cx);
    assert!(locked(cx));
    in_frame(cx, handle, |window, cx| {
        assert!(window.find("app-lock-screen").visible());
        assert!(window.try_find("settings").is_none());
        enter(window, "app-lock-password", "idle password", cx);
        window.click("app-lock-unlock", cx);
    });
    wait_for_password(cx, &lock);
    // The dialog that was open went with the lock.
    in_frame(cx, handle, |window, _| {
        assert!(window.try_find("app-lock-new-password").is_none());
        assert!(window.find("settings").visible());
    });
    advance(59, cx);
    assert!(!locked(cx), "unlock starts a full new interval");
    advance(2, cx);
    assert!(locked(cx));
    in_frame(cx, handle, |window, cx| {
        enter(window, "app-lock-password", "idle password", cx);
        window.click("app-lock-unlock", cx);
    });
    wait_for_password(cx, &lock);
    in_frame(cx, handle, |window, cx| window.press("escape", cx));
    in_frame(cx, handle, |window, cx| {
        window
            .within("settings")
            .within("group-1")
            .within("item-1")
            .click("decrement", cx);
    });
    advance(120, cx);
    assert!(!locked(cx), "setting zero disables the timeout again");
}

struct FakeScreen(std::sync::atomic::AtomicBool);

impl shellrs::app_lock::ScreenState for FakeScreen {
    fn is_locked(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[gpui_kit::test]
fn app_lock_follows_the_computers_lock_screen_once_switched_on(cx: &mut TestAppContext) {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let dir = tempfile::tempdir().unwrap();
    let (handle, lock, _) = open_locked_workspace(cx, dir.path());
    let screen = Arc::new(FakeScreen(false.into()));
    cx.update(|cx| lock.update(cx, |lock, _| lock.watch_screen(screen.clone())));
    choose_password(cx, handle, &lock, "screen password");
    app_settings(cx, handle);
    let advance = |cx: &mut TestAppContext| {
        cx.executor().advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
    };
    let locked = |cx: &mut TestAppContext| cx.update(|cx| lock.read(cx).is_locked());
    let switch = |cx: &mut TestAppContext| {
        in_frame(cx, handle, |window, cx| {
            window
                .within("settings")
                .within("group-1")
                .within("item-2")
                .click("check", cx);
        })
    };
    // 电脑锁屏时锁定 is the third row, and off until switched on.
    screen.0.store(true, Ordering::SeqCst);
    advance(cx);
    assert!(!locked(cx));
    switch(cx);
    advance(cx);
    assert!(locked(cx));

    // Back at the computer: ShellRS still asks for its own password.
    screen.0.store(false, Ordering::SeqCst);
    advance(cx);
    assert!(locked(cx));
    in_frame(cx, handle, |window, cx| {
        enter(window, "app-lock-password", "screen password", cx);
        window.click("app-lock-unlock", cx);
    });
    wait_for_password(cx, &lock);
    assert!(!locked(cx));

    switch(cx);
    screen.0.store(true, Ordering::SeqCst);
    advance(cx);
    assert!(!locked(cx));
}
