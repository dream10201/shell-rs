//! A timer's details: the dialog a click on it in 系统服务's 定时器 opens.

use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
    h_flex,
    tab::{Tab, TabBar},
    v_flex,
};
use gpui_kit::*;

use super::linux::{log_command, valid_name};
use super::model::{HostClock, ServiceCommand, Timer, file_state_label, format_moment};
use super::service_details::{DIALOG_WIDTH, Read, field, journal, render_journal, run};
use super::service_panel::{command_icon, failed_last_tag, state_color};
use crate::app::{CatalogIcon, ControlService};
use crate::i18n::t;
use crate::shared::soft_tag;
use crate::terminal::TerminalView;

/// Open the details of a timer of the host of `terminal`, as the list read
/// it, its times by `clock`; the terminal's connection reads its journal.
/// `dispatch` is the workspace's focus handle: the dialog's buttons close
/// it and dispatch there.
pub fn open_timer_dialog(
    timer: Timer,
    clock: HostClock,
    terminal: WeakEntity<TerminalView>,
    dispatch: FocusHandle,
    window: &mut Window,
    cx: &mut App,
) {
    let view = cx.new(|_| TimerDetailsView::new(timer.clone(), clock, terminal));
    window.open_dialog(cx, move |dialog, window, cx| {
        let failed = timer.failed_last().then(|| {
            let (text, color) = failed_last_tag(cx);
            soft_tag(text, color)
        });
        let title = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().truncate().child(timer.name.clone()))
                    .child(soft_tag(timer.state_label(), state_color(timer.active, cx)))
                    .children(failed),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::NORMAL)
                    .text_color(cx.theme().muted_foreground)
                    .child(timer.description.clone()),
            );
        let button = |command: ServiceCommand| {
            // 立即运行 starts the service the timer starts.
            let name = if command == ServiceCommand::Run {
                timer.unit.clone()
            } else {
                timer.name.clone()
            };
            let dispatch = dispatch.clone();
            let button = Button::new(SharedString::from(format!(
                "timer-dialog-{}",
                command.verb()
            )))
            .icon(command_icon(command))
            .label(command.label())
            .on_click(move |_, window, cx| {
                // One dialog at a time: a confirmation takes its place.
                window.close_dialog(cx);
                dispatch.dispatch_action(
                    &ControlService {
                        name: name.clone(),
                        command,
                    },
                    window,
                    cx,
                );
            });
            match command {
                ServiceCommand::Stop => button.danger(),
                ServiceCommand::Start => button.primary(),
                _ => button.outline(),
            }
        };
        let footer = DialogFooter::new()
            .justify_start()
            .children(timer.commands().iter().map(|&command| button(command)))
            .children(timer.runs_service().then(|| button(ServiceCommand::Run)))
            .children(timer.boot_command().map(button));
        dialog
            .title(title)
            // Dialog geometry is an API boundary that takes `Pixels`; the
            // width follows the interface zoom through the rem.
            .w(rems(DIALOG_WIDTH).to_pixels(window.rem_size()))
            .max_h(window.viewport_size().height * 0.9)
            // Closed by its buttons or Escape, like every dialog.
            .overlay_closable(false)
            .child(view.clone())
            .footer(footer)
    });
}

/// The dialog's body: the timer as the list read it, and its journal with
/// the service's it starts.
pub struct TimerDetailsView {
    timer: Timer,
    clock: HostClock,
    terminal: WeakEntity<TerminalView>,
    journal: Read<String>,
    /// 0 the state, 1 the journal.
    tab: usize,
    journal_scroll: ScrollHandle,
    _journal_task: Option<Task<()>>,
}

impl TimerDetailsView {
    fn new(timer: Timer, clock: HostClock, terminal: WeakEntity<TerminalView>) -> Self {
        Self {
            timer,
            clock,
            terminal,
            journal: Read::NotYet,
            tab: 0,
            journal_scroll: ScrollHandle::new(),
            _journal_task: None,
        }
    }

    fn select(&mut self, tab: usize, cx: &mut Context<Self>) {
        self.tab = tab;
        if tab == 1 && matches!(self.journal, Read::NotYet) {
            self.journal = Read::Reading;
            // What its job printed is under the service, not the timer.
            let units = [self.timer.name.as_str(), self.timer.unit.as_str()]
                .into_iter()
                .filter(|unit| valid_name(unit))
                .collect::<Vec<_>>();
            self._journal_task = Some(run(
                &self.terminal,
                log_command(&units),
                cx,
                |this, answer, _| {
                    this.journal = journal(answer);
                    // The newest lines are the last.
                    this.journal_scroll.scroll_to_bottom();
                },
            ));
        }
        cx.notify();
    }

    fn render_status(&self, cx: &App) -> AnyElement {
        let timer = &self.timer;
        let now = self.clock.now();
        let moment = |at: Option<i64>| at.map_or("—".into(), |at| format_moment(at, now).into());
        let schedule = timer
            .triggers
            .iter()
            .map(|trigger| trigger.label())
            .collect::<Vec<_>>()
            .join(" · ");
        let unit = match timer.unit_state {
            Some(state) => t!(
                "services.timer.unit_state",
                unit = timer.unit,
                state = state.label()
            )
            .into(),
            None => timer.unit.clone(),
        };
        let or_dash = |value: String| {
            if value.is_empty() {
                "—".into()
            } else {
                value
            }
        };
        v_flex()
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap_x_8()
                    .child(field(
                        "services.field.active",
                        timer.state_label().into(),
                        cx,
                    ))
                    .child(field(
                        "services.field.boot",
                        file_state_label(&timer.file_state).map_or("—".into(), Into::into),
                        cx,
                    )),
            )
            .child(field("services.field.schedule", or_dash(schedule), cx))
            .child(field("services.field.next_run", moment(timer.next), cx))
            .child(field("services.field.last_run", moment(timer.last), cx))
            .child(field("services.field.unit", or_dash(unit), cx))
            .child(field(
                "services.field.unit_file",
                or_dash(timer.path.clone()),
                cx,
            ))
            .into_any_element()
    }
}

impl Render for TimerDetailsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("timer-details")
            .test_support()
            .gap_3()
            .child(
                div().id("timer-tabs").test_support().child(
                    TabBar::new("timer-tab-bar")
                        .underline()
                        .small()
                        .selected_index(self.tab)
                        .on_click(cx.listener(|this, index: &usize, _, cx| this.select(*index, cx)))
                        .child(
                            Tab::new()
                                .icon(Icon::new(CatalogIcon::CircleDot))
                                .label(t!("services.details.tab_status")),
                        )
                        .child(
                            Tab::new()
                                .icon(Icon::new(CatalogIcon::FileText))
                                .label(t!("services.details.tab_journal")),
                        ),
                ),
            )
            .child(if self.tab == 0 {
                self.render_status(cx)
            } else {
                render_journal(&self.journal, &self.journal_scroll, cx)
            })
    }
}
