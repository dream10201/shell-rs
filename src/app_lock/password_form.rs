use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, TitleBar, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogAction, DialogClose, DialogFooter},
    form::{Field, Form},
    input::{Input, InputContentType, InputEvent, InputState},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::Range;
use zeroize::Zeroizing;

use super::{AppLock, PasswordOperation};
use crate::i18n::t;

/// The input's own handler, except that it turns the system input method off.
struct LatinInput(ElementInputHandler<InputState>);

impl InputHandler for LatinInput {
    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.0
            .selected_text_range(ignore_disabled_input, window, cx)
    }

    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.0.marked_text_range(window, cx)
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.0
            .text_for_range(range_utf16, adjusted_range, window, cx)
    }

    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0
            .replace_text_in_range(replacement_range, text, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.replace_and_mark_text_in_range(
            range_utf16,
            new_text,
            new_selected_range,
            window,
            cx,
        );
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut App) {
        self.0.unmark_text(window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.0.bounds_for_range(range_utf16, window, cx)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.0.character_index_for_point(point, window, cx)
    }

    fn text_length_utf16(&mut self, window: &mut Window, cx: &mut App) -> Option<usize> {
        self.0.text_length_utf16(window, cx)
    }

    fn accepts_text_input(&mut self, _: &mut Window, _: &mut App) -> bool {
        false
    }
}

/// Switch the keyboard to its Latin layout: macOS keeps composing whatever the handler says.
#[cfg(target_os = "macos")]
fn select_latin_keyboard() {
    use std::ffi::c_void;

    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        fn TISCopyCurrentASCIICapableKeyboardInputSource() -> *const c_void;
        fn TISSelectInputSource(source: *const c_void) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    // SAFETY: called on the main thread; the source is owned (Copy rule) and
    // released once.
    unsafe {
        let source = TISCopyCurrentASCIICapableKeyboardInputSource();
        if !source.is_null() {
            TISSelectInputSource(source);
            CFRelease(source);
        }
    }
}

struct PasswordForm {
    service: Entity<AppLock>,
    operation: PasswordOperation,
    current: Entity<InputState>,
    new: Entity<InputState>,
    repeat: Entity<InputState>,
    error: Option<SharedString>,
    working: bool,
    /// On the unlock screen, where the form has its own button.
    inline: bool,
    dismissed: bool,
    _subscriptions: Vec<Subscription>,
}

impl PasswordForm {
    fn new(
        service: Entity<AppLock>,
        operation: PasswordOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let current = cx.new(|cx| InputState::new(window, cx).masked(true));
        let new = cx.new(|cx| InputState::new(window, cx).masked(true));
        let repeat = cx.new(|cx| InputState::new(window, cx).masked(true));
        #[allow(unused_mut)]
        let mut subscriptions: Vec<Subscription> = [&current, &new, &repeat]
            .into_iter()
            .map(|input| {
                cx.subscribe_in(input, window, |this, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.commit(window, cx);
                    }
                })
            })
            .collect();
        #[cfg(target_os = "macos")]
        for input in [&current, &new, &repeat] {
            let focus = input.read(cx).focus_handle(cx);
            subscriptions.push(cx.on_focus(&focus, window, |_, _, _| select_latin_keyboard()));
        }
        Self {
            service,
            operation,
            current,
            new,
            repeat,
            error: None,
            working: false,
            inline: false,
            dismissed: false,
            _subscriptions: subscriptions,
        }
    }

    fn focus(&self, window: &mut Window, cx: &mut App) {
        let input = if self.operation == PasswordOperation::Create {
            &self.new
        } else {
            &self.current
        };
        window.focus(&input.read(cx).focus_handle(cx), cx);
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.working {
            return false;
        }
        let current = Zeroizing::new(self.current.read(cx).value().to_string());
        let new = Zeroizing::new(self.new.read(cx).value().to_string());
        let sets_password = self.operation != PasswordOperation::Unlock;
        self.error = if sets_password && new.chars().count() < super::SHORTEST_PASSWORD {
            Some(t!("app_lock.too_short"))
        } else if sets_password && new.as_str() != self.repeat.read(cx).value().as_str() {
            Some(t!("app_lock.mismatch"))
        } else if self.operation != PasswordOperation::Create && current.is_empty() {
            Some(t!("app_lock.enter_password"))
        } else {
            None
        };
        if self.error.is_some() {
            cx.notify();
            return false;
        }
        let job = self.service.update(cx, |service, cx| {
            service.submit(self.operation, current, new, cx)
        });
        self.working = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = job.await;
            this.update_in(cx, |this, window, cx| {
                this.working = false;
                match result {
                    Ok(()) => {
                        for input in [&this.current, &this.new, &this.repeat] {
                            input.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                                input.set_masked(true, window, cx);
                            });
                        }
                        if !this.inline && !this.dismissed {
                            window.close_dialog(cx);
                        }
                    }
                    Err(error) => {
                        this.error = Some(error);
                        this.current.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                            input.set_masked(true, window, cx);
                        });
                        this.focus(window, cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        false
    }
}

impl Render for PasswordForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = |state: &Entity<InputState>, id: &'static str| {
            Input::new(state)
                .id(id)
                .mask_toggle()
                .content_type(InputContentType::Password)
                .disabled(self.working)
        };
        let inputs = [&self.current, &self.new, &self.repeat].map(|input| {
            let focus = input.read(cx).focus_handle(cx);
            (input.clone(), focus)
        });
        v_flex()
            .gap_3()
            .child(
                Form::new()
                    .when(self.operation != PasswordOperation::Create, |form| {
                        form.child(
                            Field::new()
                                .label(t!("app_lock.current_password"))
                                .child(input(&self.current, "app-lock-password")),
                        )
                    })
                    .when(self.operation != PasswordOperation::Unlock, |form| {
                        form.child(
                            Field::new()
                                .label(t!("app_lock.new_password"))
                                .description(t!("app_lock.password_help"))
                                .child(input(&self.new, "app-lock-new-password")),
                        )
                        .child(
                            Field::new()
                                .label(t!("app_lock.repeat_password"))
                                .child(input(&self.repeat, "app-lock-repeat-password")),
                        )
                    }),
            )
            // Painted after the inputs, so this handler is the one the window keeps.
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, cx| {
                        for (input, focus) in inputs {
                            let handler = LatinInput(ElementInputHandler::new(bounds, input));
                            window.handle_input(&focus, handler, cx);
                        }
                    },
                )
                .absolute()
                .size_0(),
            )
            .when_some(self.error.clone(), |view, error| {
                view.child(
                    div()
                        .id("app-lock-error")
                        .test_support()
                        .role(Role::Alert)
                        .aria_label(error.clone())
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .when(self.inline, |view| {
                view.child(
                    Button::new("app-lock-unlock")
                        .primary()
                        .label(if self.operation == PasswordOperation::Create {
                            t!("app_lock.setup_commit")
                        } else {
                            t!("app_lock.unlock")
                        })
                        .loading(self.working)
                        .disabled(self.working)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.commit(window, cx);
                        })),
                )
            })
    }
}

/// Open the dialog that changes the password.
pub fn open_password_dialog(service: Entity<AppLock>, window: &mut Window, cx: &mut App) {
    if service.read(cx).is_locked() {
        return;
    }
    let operation = PasswordOperation::Change;
    let title = t!("app_lock.change_title");
    let commit_label = t!("app_lock.change_commit");
    let form = cx.new(|cx| PasswordForm::new(service, operation, window, cx));
    window.open_dialog(cx, {
        let form = form.clone();
        move |dialog, _, cx| {
            let working = form.read(cx).working;
            let commit = form.clone();
            let cancel = form.clone();
            let close = form.clone();
            dialog
                .title(title.clone())
                .overlay_closable(false)
                .keyboard(!working)
                .close_button(!working)
                .child(form.clone())
                .footer(
                    DialogFooter::new()
                        .child(DialogClose::new().trigger(move |button| {
                            button.label(t!("common.cancel")).disabled(working)
                        }))
                        .child(
                            DialogAction::new().child(
                                Button::new("app-lock-commit")
                                    .primary()
                                    .label(commit_label.clone())
                                    .disabled(working)
                                    .loading(working),
                            ),
                        ),
                )
                .on_ok(move |_, window, cx| commit.update(cx, |form, cx| form.commit(window, cx)))
                .on_cancel(move |_, _, cx| !cancel.read(cx).working)
                .on_close(move |_, _, cx| close.update(cx, |form, _| form.dismissed = true))
        }
    });
    form.update(cx, |form, cx| form.focus(window, cx));
}

/// The sole visible content while locked. There is no dismiss path to the workspace.
pub struct UnlockScreen {
    service: Entity<AppLock>,
    form: Entity<PasswordForm>,
    /// Around everything on the screen that takes the keyboard.
    scope: FocusHandle,
    _subscription: Subscription,
}

impl UnlockScreen {
    pub fn new(service: Entity<AppLock>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let operation = |service: &AppLock| {
            if service.needs_password() {
                PasswordOperation::Create
            } else {
                PasswordOperation::Unlock
            }
        };
        let form = cx.new(|cx| {
            let mut form =
                PasswordForm::new(service.clone(), operation(service.read(cx)), window, cx);
            form.inline = true;
            form
        });
        // A password just chosen is the one asked for from then on.
        let subscription = cx.observe(&service, {
            let form = form.clone();
            move |_, service, cx| {
                let operation = operation(service.read(cx));
                form.update(cx, |form, cx| {
                    if form.operation != operation {
                        form.operation = operation;
                        cx.notify();
                    }
                });
                cx.notify();
            }
        });
        Self {
            service,
            form,
            scope: cx.focus_handle(),
            _subscription: subscription,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.form.update(cx, |form, cx| form.focus(window, cx));
    }

    /// Whether the keyboard is on the screen, not on what it covers.
    pub fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.scope.contains_focused(window, cx)
    }
}

impl Render for UnlockScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (title, description) = if self.service.read(cx).needs_password() {
            (t!("app_lock.setup_title"), t!("app_lock.setup_description"))
        } else {
            (t!("app_lock.locked"), t!("app_lock.unlock_description"))
        };
        v_flex()
            .id("app-lock-screen")
            .test_support()
            .track_focus(&self.scope)
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child("ShellRS"))
            .child(
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .p_6()
                    .child(
                        v_flex()
                            .w_80()
                            .max_w_full()
                            .gap_4()
                            .child(
                                Icon::new(crate::app::CatalogIcon::Lock)
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(description),
                            )
                            .child(self.form.clone()),
                    ),
            )
    }
}
