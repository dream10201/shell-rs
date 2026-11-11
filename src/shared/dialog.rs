//! The pieces every form dialog is put together from.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::{DialogAction, DialogButtonProps, DialogClose, DialogFooter},
    h_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::i18n::t;

/// A form dialog's footer: 取消, and the button that commits the form.
pub fn commit_footer(id: &'static str, label: impl Into<SharedString>) -> DialogFooter {
    DialogFooter::new()
        .child(DialogClose::new().trigger(|button| button.label(t!("common.cancel"))))
        .child(DialogAction::new().child(Button::new(id).primary().label(label)))
}

/// How long a form's error stays when nobody puts it away.
const FORM_ERROR_TIME: Duration = Duration::from_secs(6);

/// The error of the form in front, until it is put away; `shown` tells one
/// showing from the next.
#[derive(Default)]
struct FormError {
    message: Option<SharedString>,
    shown: usize,
}

impl Global for FormError {}

/// Say why a form was not accepted, over the dialog: every add and edit
/// dialog says it this way, not in a line under the form. Its text has the
/// `form-error` id, with the message as its label. A newer error takes the
/// last one's place; a click puts it away, so does the dialog when it
/// closes (each dialog calls `dismiss_form_error` from its `on_close`), and
/// so does time.
///
/// Not one of the window's notifications: those are drawn under a dialog's
/// backdrop, where an error about the dialog would be dimmed and out of
/// reach. The window's content draws this one with [`form_error_layer`].
pub fn show_form_error(error: impl Into<SharedString>, window: &mut Window, cx: &mut App) {
    let state = cx.default_global::<FormError>();
    state.message = Some(error.into());
    state.shown += 1;
    let shown = state.shown;
    window.refresh();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(FORM_ERROR_TIME).await;
        cx.update(|cx| {
            let state = cx.default_global::<FormError>();
            if state.shown == shown && state.message.take().is_some() {
                cx.refresh_windows();
            }
        });
    })
    .detach();
}

/// Take a form's error away once its dialog has closed: the error is about
/// a form that is gone.
pub fn dismiss_form_error(window: &mut Window, cx: &mut App) {
    if cx.default_global::<FormError>().message.take().is_some() {
        window.refresh();
    }
}

/// The form error that is showing, for the window's content to draw: above
/// any dialog, below an open menu.
pub fn form_error_layer(cx: &App) -> Option<impl IntoElement + use<>> {
    let error = cx.try_global::<FormError>()?.message.clone()?;
    let theme = cx.theme();
    let toast = h_flex()
        .id("form-error-toast")
        .occlude()
        .items_start()
        .gap_2()
        .w(rems(22.))
        .p_3()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .text_color(theme.popover_foreground)
        .shadow_md()
        .cursor_pointer()
        .on_click(|_, window, cx| dismiss_form_error(window, cx))
        .child(
            Icon::new(IconName::CircleX)
                .text_color(theme.danger)
                .mt_0p5(),
        )
        .child(
            div()
                .id("form-error")
                .test_support()
                .aria_label(error.clone())
                .flex_1()
                .min_w_0()
                .text_sm()
                .child(error),
        );
    Some(
        deferred(div().absolute().top_4().right_4().child(toast))
            .with_priority(gpui_kit::base::POPUP_PRIORITY - 1),
    )
}

/// A port field's value, when it is one: a number from 1 to 65535.
pub fn parse_port(port: &str) -> Option<u16> {
    port.trim().parse::<u16>().ok().filter(|port| *port != 0)
}

/// What runs when the user confirms a deletion.
pub type DeleteHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// The confirmation every deletion of a saved thing shares: 删除“名称”？ with
/// a danger button. `description` says what goes with it, when anything does.
pub fn confirm_delete(
    name: &str,
    description: Option<SharedString>,
    on_delete: DeleteHandler,
    window: &mut Window,
    cx: &mut App,
) {
    confirm_danger(
        t!("common.delete_title", name = name),
        description,
        t!("common.delete"),
        on_delete,
        window,
        cx,
    );
}

/// A confirmation of something that cannot be taken back: `title` asks,
/// `description` says what follows, and the danger button names the verb.
pub fn confirm_danger(
    title: SharedString,
    description: Option<SharedString>,
    verb: impl Into<SharedString>,
    on_ok: DeleteHandler,
    window: &mut Window,
    cx: &mut App,
) {
    confirm_danger_with_cancel(
        title,
        description,
        verb,
        on_ok,
        Rc::new(|_, _| {}),
        window,
        cx,
    );
}

/// The same confirmation with an owner callback to discard a prepared command.
pub fn confirm_danger_with_cancel(
    title: SharedString,
    description: Option<SharedString>,
    verb: impl Into<SharedString>,
    on_ok: DeleteHandler,
    on_cancel: DeleteHandler,
    window: &mut Window,
    cx: &mut App,
) {
    let verb = verb.into();
    window.open_alert_dialog(cx, move |alert, _, _| {
        alert
            .title(title.clone())
            .when_some(description.clone(), |alert, description| {
                alert.description(description)
            })
            .button_props(
                DialogButtonProps::default()
                    .ok_text(verb.clone())
                    .ok_variant(ButtonVariant::Danger)
                    .cancel_text(t!("common.cancel")),
            )
            .show_cancel(true)
            .on_ok({
                let on_ok = on_ok.clone();
                move |_, window, cx| {
                    on_ok(window, cx);
                    true
                }
            })
            .on_cancel({
                let on_cancel = on_cancel.clone();
                move |_, window, cx| {
                    on_cancel(window, cx);
                    true
                }
            })
    });
}

#[cfg(test)]
mod tests {
    use super::parse_port;

    #[test]
    fn a_port_field_is_a_number_from_1_to_65535() {
        assert_eq!(parse_port("22"), Some(22));
        assert_eq!(parse_port(" 8080 "), Some(8080));
        assert_eq!(parse_port("65535"), Some(65535));
        for port in ["0", "65536", "ssh", ""] {
            assert_eq!(parse_port(port), None, "{port:?}");
        }
    }
}
