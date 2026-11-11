//! Own the startup boundary: no workspace (and no automatic connections,
//! sync or saved terminals) is created until the password is accepted.
use crate::app_lock::{AppLock, UnlockScreen};
use crate::instance::{InstanceServer, OpenLink};
use gpui_kit::component::WindowExt as _;
use gpui_kit::*;
use std::time::Duration;

/// Paint order of the unlock screen: over dialogs, notifications and menus.
const LOCK_PRIORITY: usize = gpui_kit::base::POPUP_PRIORITY + 1000;

type WorkspaceFactory =
    Box<dyn FnOnce(Option<InstanceServer>, Vec<OpenLink>, &mut Window, &mut App) -> AnyView>;

pub struct AppShell {
    lock: Entity<AppLock>,
    unlock: Entity<UnlockScreen>,
    content: Option<AnyView>,
    factory: Option<WorkspaceFactory>,
    server: Option<InstanceServer>,
    pending_links: Vec<OpenLink>,
    previous_focus: Option<FocusHandle>,
    was_locked: bool,
    _subscription: Subscription,
}

impl AppShell {
    pub fn new(
        lock: Entity<AppLock>,
        server: Option<InstanceServer>,
        factory: impl FnOnce(Option<InstanceServer>, Vec<OpenLink>, &mut Window, &mut App) -> AnyView
        + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let locked = lock.read(cx).is_locked();
        let unlock = cx.new(|cx| UnlockScreen::new(lock.clone(), window, cx));
        let subscription =
            cx.observe_in(&lock, window, |this, _, window, cx| this.sync(window, cx));
        let mut this = Self {
            lock,
            unlock,
            content: None,
            factory: Some(Box::new(factory)),
            server,
            pending_links: Vec::new(),
            previous_focus: None,
            was_locked: locked,
            _subscription: subscription,
        };
        this.sync(window, cx);
        // Keep second launches working while the workspace does not exist.
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if !this
                    .update_in(cx, |this, window, cx| {
                        let Some(server) = &this.server else {
                            return false;
                        };
                        if let Some(links) = server.take_activation() {
                            this.pending_links.extend(links);
                            crate::app::bring_forward(window, cx);
                        }
                        if crate::app::take_system_hotkey_press() {
                            if window.is_window_active() {
                                crate::app::send_back(window, cx);
                            } else {
                                crate::app::bring_forward(window, cx);
                            }
                        }
                        true
                    })
                    .unwrap_or(false)
                {
                    break;
                }
            }
        })
        .detach();
        this
    }

    fn create_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(server) = &self.server
            && let Some(links) = server.take_activation()
        {
            self.pending_links.extend(links);
        }
        if let Some(factory) = self.factory.take() {
            self.content = Some(factory(
                self.server.take(),
                std::mem::take(&mut self.pending_links),
                window,
                cx,
            ));
        }
    }

    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lock = self.lock.read(cx);
        let locked = lock.is_locked();
        if self.content.is_none() && !locked {
            self.create_workspace(window, cx);
        }
        if locked {
            if !self.was_locked {
                // What the root draws above the content would show over
                // the unlock screen, and a dialog keeps the keyboard.
                let covered = window.has_active_dialog(cx);
                window.close_all_dialogs(cx);
                if window.has_active_sheet(cx) {
                    window.close_sheet(cx);
                }
                window.clear_notifications(cx);
                crate::shared::dismiss_form_error(window, cx);
                // What a dialog held goes with it: unlocking then leaves
                // the keyboard to the workspace to place.
                self.previous_focus = (!covered).then(|| window.focused(cx)).flatten();
            }
            self.unlock
                .update(cx, |screen, cx| screen.focus(window, cx));
        } else if self.was_locked
            && let Some(focus) = self.previous_focus.take()
        {
            window.focus(&focus, cx);
        }
        self.was_locked = locked;
        cx.notify();
    }
}

impl Render for AppShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = if self.lock.read(cx).is_locked() {
            // The keyboard stays on the unlock screen: a dialog closed by
            // locking gives it back to what had it before.
            if !self.unlock.read(cx).contains_focus(window, cx) {
                cx.defer_in(window, |this, window, cx| {
                    if this.lock.read(cx).is_locked() {
                        this.unlock
                            .update(cx, |screen, cx| screen.focus(window, cx));
                    }
                });
            }
            // Above whatever the root still draws over the content: a
            // dialog on its way out, a notification that comes while locked.
            deferred(
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .child(self.unlock.clone()),
            )
            .with_priority(LOCK_PRIORITY)
            .into_any_element()
        } else {
            self.content
                .clone()
                .expect("workspace after unlock")
                .into_any_element()
        };
        div()
            .size_full()
            .child(crate::app_lock::activity_tracker(cx))
            .child(content)
    }
}

/// A failed single-instance reservation must not open a second writable
/// workspace. Show the reason in the window rather than terminating abruptly.
pub struct StartupFailure {
    message: SharedString,
}

impl StartupFailure {
    pub fn new(error: &std::io::Error) -> Self {
        Self {
            message: crate::i18n::t!("app_lock.start_failed", error = error),
        }
    }
}

impl Render for StartupFailure {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::{ActiveTheme as _, TitleBar, v_flex};
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child("ShellRS"))
            .child(
                v_flex()
                    .flex_1()
                    .justify_center()
                    .items_center()
                    .p_6()
                    .child(div().max_w_96().text_sm().child(self.message.clone())),
            )
    }
}
