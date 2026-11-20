//! What 系统服务 reads off a host, and how its two lists, tabs, groups and
//! search pick from it.

use std::time::Instant;

use gpui_kit::SharedString;

use crate::i18n::{t, tn};

/// Where a service's unit file lives, which the list groups by.
const CUSTOM_UNITS: &str = "/etc/systemd/system/";

/// What a service is doing, as systemd's `ActiveState` says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveState {
    Active,
    Reloading,
    Inactive,
    Failed,
    Activating,
    Deactivating,
    Other,
}

impl ActiveState {
    pub fn parse(state: &str) -> Self {
        match state {
            "active" => ActiveState::Active,
            "reloading" => ActiveState::Reloading,
            "inactive" => ActiveState::Inactive,
            "failed" => ActiveState::Failed,
            "activating" => ActiveState::Activating,
            "deactivating" => ActiveState::Deactivating,
            _ => ActiveState::Other,
        }
    }

    pub fn label(self) -> SharedString {
        match self {
            ActiveState::Active => t!("services.active.active"),
            ActiveState::Reloading => t!("services.active.reloading"),
            ActiveState::Inactive | ActiveState::Other => t!("services.active.inactive"),
            ActiveState::Failed => t!("services.active.failed"),
            ActiveState::Activating => t!("services.active.activating"),
            ActiveState::Deactivating => t!("services.active.deactivating"),
        }
    }

    /// Which tab it is under.
    pub fn kind(self) -> ServiceKind {
        match self {
            ActiveState::Active
            | ActiveState::Reloading
            | ActiveState::Activating
            | ActiveState::Deactivating => ServiceKind::Running,
            ActiveState::Failed => ServiceKind::Failed,
            ActiveState::Inactive | ActiveState::Other => ServiceKind::Stopped,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceKind {
    Running,
    Stopped,
    Failed,
}

/// The finer state under `ActiveState`, as systemd's `SubState` says: 「已退出」
/// for a one-shot service that did its work.
pub fn sub_state_label(state: &str) -> SharedString {
    match state {
        "running" => t!("services.sub.running"),
        "exited" => t!("services.sub.exited"),
        "dead" => t!("services.sub.dead"),
        "failed" => t!("services.sub.failed"),
        "waiting" => t!("services.sub.waiting"),
        "listening" => t!("services.sub.listening"),
        "reload" => t!("services.sub.reload"),
        "auto-restart" => t!("services.sub.auto_restart"),
        "condition" => t!("services.sub.condition"),
        "elapsed" => t!("services.sub.elapsed"),
        state if state.starts_with("start") => t!("services.sub.starting"),
        state if state.starts_with("stop") || state.starts_with("final") => {
            t!("services.sub.stopping")
        }
        state => state.to_owned().into(),
    }
}

/// Whether it starts at boot, as systemd's `UnitFileState` says; `None`
/// with nothing to say.
pub fn file_state_label(state: &str) -> Option<SharedString> {
    Some(match state {
        "enabled" => t!("services.file.enabled"),
        "enabled-runtime" => t!("services.file.enabled_runtime"),
        "disabled" => t!("services.file.disabled"),
        "static" => t!("services.file.static"),
        "masked" | "masked-runtime" => t!("services.file.masked"),
        "generated" => t!("services.file.generated"),
        "indirect" => t!("services.file.indirect"),
        "linked" | "linked-runtime" => t!("services.file.linked"),
        "transient" => t!("services.file.transient"),
        "alias" => t!("services.file.alias"),
        "bad" => t!("services.file.bad"),
        _ => return None,
    })
}

/// Whether the unit file was found and read, as systemd's `LoadState` says.
pub fn load_state_label(state: &str) -> SharedString {
    match state {
        "loaded" => t!("services.load.loaded"),
        "not-found" => t!("services.load.not_found"),
        "masked" => t!("services.load.masked"),
        "error" => t!("services.load.error"),
        "bad-setting" => t!("services.load.bad_setting"),
        state => state.to_owned().into(),
    }
}

/// How the machine as a whole is doing, as `systemctl is-system-running`
/// says: 「降级运行」 when some unit failed.
pub fn system_state_label(state: &str) -> SharedString {
    match state {
        "running" => t!("services.system.running"),
        "degraded" => t!("services.system.degraded"),
        "starting" => t!("services.system.starting"),
        "initializing" => t!("services.system.initializing"),
        "maintenance" => t!("services.system.maintenance"),
        "stopping" => t!("services.system.stopping"),
        "offline" => t!("services.system.offline"),
        state => state.to_owned().into(),
    }
}

/// What can be done to a service or a timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServiceCommand {
    Start,
    Stop,
    Restart,
    /// Start it at boot.
    Enable,
    /// No longer start it at boot.
    Disable,
    /// Start the service a timer starts, now rather than on schedule,
    /// without waiting for it to finish: a timer's job can take hours.
    Run,
}

impl ServiceCommand {
    /// Its name in element ids: 「service-stop:nginx.service」.
    pub fn verb(self) -> &'static str {
        match self {
            ServiceCommand::Start => "start",
            ServiceCommand::Stop => "stop",
            ServiceCommand::Restart => "restart",
            ServiceCommand::Enable => "enable",
            ServiceCommand::Disable => "disable",
            ServiceCommand::Run => "run",
        }
    }

    /// What `systemctl` is told before the unit's name.
    pub fn systemctl(self) -> &'static str {
        match self {
            ServiceCommand::Run => "start --no-block",
            command => command.verb(),
        }
    }

    /// The button: 「停止」「禁用开机启动」.
    pub fn label(self) -> SharedString {
        match self {
            ServiceCommand::Start => t!("services.command.start"),
            ServiceCommand::Stop => t!("services.command.stop"),
            ServiceCommand::Restart => t!("services.command.restart"),
            ServiceCommand::Enable => t!("services.command.enable"),
            ServiceCommand::Disable => t!("services.command.disable"),
            ServiceCommand::Run => t!("services.command.run"),
        }
    }
}

/// Whether a unit file at `path` is the administrator's, in
/// `/etc/systemd/system`, which the lists group by.
pub fn custom_unit(path: &str) -> bool {
    path.starts_with(CUSTOM_UNITS)
}

/// Enable or disable a unit whose `UnitFileState` is `file_state`, when it
/// can be either.
fn boot_command(file_state: &str) -> Option<ServiceCommand> {
    match file_state {
        "enabled" | "enabled-runtime" => Some(ServiceCommand::Disable),
        "disabled" => Some(ServiceCommand::Enable),
        _ => None,
    }
}

/// A service as the list shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    /// 「nginx.service」.
    pub name: String,
    pub description: String,
    pub active: ActiveState,
    /// systemd's `UnitFileState`: 「enabled」「static」, empty without a
    /// unit file.
    pub file_state: String,
    /// Its unit file is the administrator's, in `/etc/systemd/system`.
    pub custom: bool,
    /// The timers that start it, by name: a job run on a schedule, which
    /// is stopped between runs.
    pub timers: Vec<String>,
}

impl Service {
    pub fn new(name: &str, description: &str, active: &str, file_state: &str, path: &str) -> Self {
        Self {
            name: name.to_owned(),
            description: description.to_owned(),
            active: ActiveState::parse(active),
            file_state: file_state.to_owned(),
            custom: custom_unit(path),
            timers: Vec::new(),
        }
    }

    /// The commands its buttons offer, the one that matters first: stop
    /// and restart while it runs, start otherwise.
    pub fn commands(&self) -> &'static [ServiceCommand] {
        match self.active.kind() {
            ServiceKind::Running => &[ServiceCommand::Stop, ServiceCommand::Restart],
            ServiceKind::Stopped | ServiceKind::Failed => &[ServiceCommand::Start],
        }
    }

    /// Enable or disable, when its unit file can be either.
    pub fn boot_command(&self) -> Option<ServiceCommand> {
        boot_command(&self.file_state)
    }

    fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || self.name.to_lowercase().contains(query)
            || self.description.to_lowercase().contains(query)
    }
}

/// What starts a timer's clock: systemd's `On…Sec=` settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerBase {
    /// `OnActiveSec=`: the timer starting.
    Active,
    /// `OnBootSec=`.
    Boot,
    /// `OnStartupSec=`: systemd starting.
    Startup,
    /// `OnUnitActiveSec=`: the unit it starts starting.
    UnitActive,
    /// `OnUnitInactiveSec=`: the unit it starts stopping.
    UnitInactive,
}

/// What a timer goes off on.
#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    /// `OnCalendar=`, as systemd writes it back: 「*-*-* 00:00:00」.
    Calendar(String),
    /// A time span, as systemd writes it (「15min」), after something.
    After(TriggerBase, String),
}

impl Trigger {
    /// 「*-*-* 00:00:00」, 「开机 15min 后」.
    pub fn label(&self) -> SharedString {
        match self {
            Trigger::Calendar(spec) => spec.clone().into(),
            Trigger::After(base, span) => match base {
                TriggerBase::Active => t!("services.trigger.active", time = span),
                TriggerBase::Boot => t!("services.trigger.boot", time = span),
                TriggerBase::Startup => t!("services.trigger.startup", time = span),
                TriggerBase::UnitActive => t!("services.trigger.unit_active", time = span),
                TriggerBase::UnitInactive => t!("services.trigger.unit_inactive", time = span),
            },
        }
    }
}

/// A timer as its list shows it. Times are seconds since the epoch, by the
/// host's clock.
#[derive(Clone, Debug, PartialEq)]
pub struct Timer {
    /// 「logrotate.timer」.
    pub name: String,
    pub description: String,
    pub active: ActiveState,
    /// systemd's `SubState`: 「waiting」 for its time, 「running」 while the
    /// unit it started runs.
    pub sub: String,
    pub file_state: String,
    pub custom: bool,
    /// Its unit file, empty without one.
    pub path: String,
    /// The unit it starts: 「logrotate.service」.
    pub unit: String,
    pub triggers: Vec<Trigger>,
    /// When it goes off next; `None` while it is not waiting to.
    pub next: Option<i64>,
    /// When it went off last, if the host knows.
    pub last: Option<i64>,
    /// What the unit it starts is doing, when the list has that unit.
    pub unit_state: Option<ActiveState>,
}

impl Timer {
    /// Which tab it is under: 失败 when the unit it starts failed last
    /// time, whatever the timer itself is doing, for that is what needs
    /// looking at.
    pub fn kind(&self) -> ServiceKind {
        if self.failed_last() {
            ServiceKind::Failed
        } else {
            self.active.kind()
        }
    }

    /// Whether the unit it starts failed last time it ran.
    pub fn failed_last(&self) -> bool {
        self.unit_state == Some(ActiveState::Failed)
    }

    /// 「等待中」: what it is doing, by `SubState` while it is active.
    pub fn state_label(&self) -> SharedString {
        match self.active {
            ActiveState::Active => sub_state_label(&self.sub),
            active => active.label(),
        }
    }

    /// Stop it while it is active, start it otherwise.
    pub fn commands(&self) -> &'static [ServiceCommand] {
        match self.active.kind() {
            ServiceKind::Running => &[ServiceCommand::Stop],
            ServiceKind::Stopped | ServiceKind::Failed => &[ServiceCommand::Start],
        }
    }

    /// Enable or disable, when its unit file can be either.
    pub fn boot_command(&self) -> Option<ServiceCommand> {
        boot_command(&self.file_state)
    }

    /// Whether 立即运行 can start what it starts: a service.
    pub fn runs_service(&self) -> bool {
        self.unit.ends_with(".service")
    }

    /// 「下次 4 小时后 · 上次 19 小时前」 at `now`; `None` when neither is
    /// known. A next time gone by since the list was read is due.
    pub fn times(&self, now: i64) -> Option<String> {
        let next = self.next.map(|next| {
            if next <= now {
                t!("services.timer.due")
            } else {
                t!("services.timer.next", when = format_from_now(next, now))
            }
        });
        let last = self
            .last
            .map(|last| t!("services.timer.last", when = format_from_now(last, now)));
        let times = [next, last].into_iter().flatten().collect::<Vec<_>>();
        (!times.is_empty()).then(|| times.join(" · "))
    }

    fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || self.name.to_lowercase().contains(query)
            || self.description.to_lowercase().contains(query)
            || self.unit.to_lowercase().contains(query)
    }
}

/// When `at` is, from `now`: 「4 小时后」, 「19 小时前」, 「刚刚」.
pub fn format_from_now(at: i64, now: i64) -> SharedString {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    let ahead = at - now;
    if ahead > 0 {
        match ahead {
            ..MINUTE => t!("services.when.within_a_minute"),
            MINUTE..HOUR => tn!("services.when.in_minutes", ahead / MINUTE),
            HOUR..DAY => tn!("services.when.in_hours", ahead / HOUR),
            _ => tn!("services.when.in_days", ahead / DAY),
        }
    } else {
        let ago = -ahead;
        match ago {
            ..MINUTE => t!("services.when.just_now"),
            MINUTE..HOUR => tn!("services.when.minutes_ago", ago / MINUTE),
            HOUR..DAY => tn!("services.when.hours_ago", ago / HOUR),
            _ => tn!("services.when.days_ago", ago / DAY),
        }
    }
}

/// `at` on this computer's clock, and from `now`: 「2026-11-20
/// 08:00:00（16 小时后）」.
pub fn format_moment(at: i64, now: i64) -> SharedString {
    let time = chrono::DateTime::from_timestamp(at, 0)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_default();
    t!(
        "services.timer.moment",
        time = time,
        when = format_from_now(at, now)
    )
}

/// The host's clock, as a reading found it: the timers' times are by it,
/// and this computer's may be off.
#[derive(Clone, Copy, Debug)]
pub struct HostClock {
    /// What it said, in seconds since the epoch.
    said: i64,
    /// When that was, here.
    at: Instant,
}

impl HostClock {
    /// The host's clock that said `said`, now; this computer's when the
    /// host did not say.
    pub fn new(said: Option<i64>) -> Self {
        Self {
            said: said.unwrap_or_else(|| chrono::Utc::now().timestamp()),
            at: Instant::now(),
        }
    }

    /// What the host's clock says now.
    pub fn now(&self) -> i64 {
        self.said + self.at.elapsed().as_secs() as i64
    }
}

/// The two lists over the services' tabs: the services, and the timers
/// that start some of them on a schedule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UnitKind {
    #[default]
    Service,
    Timer,
}

impl UnitKind {
    pub const ALL: [UnitKind; 2] = [UnitKind::Service, UnitKind::Timer];

    pub fn label(self) -> SharedString {
        match self {
            UnitKind::Service => t!("services.kind.service"),
            UnitKind::Timer => t!("services.kind.timer"),
        }
    }
}

/// The tabs over the list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ServiceFilter {
    #[default]
    All,
    Running,
    Stopped,
    Failed,
}

impl ServiceFilter {
    pub const ALL: [ServiceFilter; 4] = [
        ServiceFilter::All,
        ServiceFilter::Running,
        ServiceFilter::Stopped,
        ServiceFilter::Failed,
    ];

    pub fn label(self) -> SharedString {
        match self {
            ServiceFilter::All => t!("services.filter.all"),
            ServiceFilter::Running => t!("services.filter.running"),
            ServiceFilter::Stopped => t!("services.filter.stopped"),
            ServiceFilter::Failed => t!("services.filter.failed"),
        }
    }

    fn lets_through(self, kind: ServiceKind) -> bool {
        match self {
            ServiceFilter::All => true,
            ServiceFilter::Running => kind == ServiceKind::Running,
            ServiceFilter::Stopped => kind == ServiceKind::Stopped,
            ServiceFilter::Failed => kind == ServiceKind::Failed,
        }
    }
}

/// A line of a list: a group's heading, or a service or a timer by its
/// place in `ServiceTable::services` or `ServiceTable::timers`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceRow {
    Group {
        kind: UnitKind,
        custom: bool,
        count: usize,
    },
    Service(usize),
    Timer(usize),
}

/// One reading of a host's services and timers: the services by name, the
/// timers by when they go off next.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServiceTable {
    services: Vec<Service>,
    timers: Vec<Timer>,
    /// 「systemd 249」.
    version: Option<String>,
    /// `systemctl is-system-running`: 「degraded」.
    state: Option<String>,
    /// The host's clock as it was read, in seconds since the epoch.
    now: Option<i64>,
}

impl ServiceTable {
    /// Each timer learns what the service it starts is doing, and each
    /// service which timers start it.
    pub fn new(
        mut services: Vec<Service>,
        mut timers: Vec<Timer>,
        version: Option<String>,
        state: Option<String>,
        now: Option<i64>,
    ) -> Self {
        services.sort_by(|a, b| a.name.cmp(&b.name));
        services.dedup_by(|a, b| a.name == b.name);
        timers.sort_by(|a, b| a.name.cmp(&b.name));
        timers.dedup_by(|a, b| a.name == b.name);
        // As `systemctl list-timers` has them: the soonest first, those
        // not waiting last.
        timers.sort_by_key(|timer| (timer.next.is_none(), timer.next));
        for timer in &mut timers {
            let service = services
                .iter_mut()
                .find(|service| service.name == timer.unit);
            if let Some(service) = service {
                timer.unit_state = Some(service.active);
                service.timers.push(timer.name.clone());
            }
        }
        Self {
            services,
            timers,
            version,
            state,
            now,
        }
    }

    pub fn services(&self) -> &[Service] {
        &self.services
    }

    pub fn timers(&self) -> &[Timer] {
        &self.timers
    }

    pub fn service(&self, name: &str) -> Option<&Service> {
        self.services.iter().find(|service| service.name == name)
    }

    pub fn timer(&self, name: &str) -> Option<&Timer> {
        self.timers.iter().find(|timer| timer.name == name)
    }

    /// What the host's clock said when it was read.
    pub fn now(&self) -> Option<i64> {
        self.now
    }

    /// 「systemd 249 · 降级运行」.
    pub fn summary(&self) -> String {
        [
            self.version.clone(),
            self.state
                .as_deref()
                .map(|state| system_state_label(state).into()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
    }

    /// Whether some unit failed, so the machine is not quite well.
    pub fn degraded(&self) -> bool {
        self.state.as_deref() == Some("degraded")
    }

    /// The services or timers `query` finds (in the name or description,
    /// a timer's also in what it starts, ignoring case): for each, its
    /// place in its list, whether it is the administrator's, and its tab.
    fn found(&self, kind: UnitKind, query: &str) -> Vec<(usize, bool, ServiceKind)> {
        let query = query.trim().to_lowercase();
        match kind {
            UnitKind::Service => self
                .services
                .iter()
                .enumerate()
                .filter(|(_, service)| service.matches(&query))
                .map(|(index, service)| (index, service.custom, service.active.kind()))
                .collect(),
            UnitKind::Timer => self
                .timers
                .iter()
                .enumerate()
                .filter(|(_, timer)| timer.matches(&query))
                .map(|(index, timer)| (index, timer.custom, timer.kind()))
                .collect(),
        }
    }

    /// How many of `kind` `query` finds under each tab, in the tabs'
    /// order.
    pub fn counts(&self, kind: UnitKind, query: &str) -> [usize; 4] {
        let found = self.found(kind, query);
        ServiceFilter::ALL.map(|filter| {
            found
                .iter()
                .filter(|(_, _, tab)| filter.lets_through(*tab))
                .count()
        })
    }

    /// The list of `kind` under `filter` for `query`: the administrator's
    /// own, then the rest, each group under its heading; a group with
    /// none is left out.
    pub fn rows(&self, kind: UnitKind, filter: ServiceFilter, query: &str) -> Vec<ServiceRow> {
        let found = self.found(kind, query);
        let mut rows = Vec::new();
        for custom in [true, false] {
            let group: Vec<usize> = found
                .iter()
                .filter(|(_, own, tab)| *own == custom && filter.lets_through(*tab))
                .map(|(index, _, _)| *index)
                .collect();
            if !group.is_empty() {
                rows.push(ServiceRow::Group {
                    kind,
                    custom,
                    count: group.len(),
                });
                rows.extend(group.into_iter().map(|index| match kind {
                    UnitKind::Service => ServiceRow::Service(index),
                    UnitKind::Timer => ServiceRow::Timer(index),
                }));
            }
        }
        rows
    }
}

/// A service's state in full, for its details, as `systemctl show` says.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServiceStatus {
    pub description: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub file_state: String,
    pub main_pid: Option<u32>,
    /// Bytes; `None` without memory accounting.
    pub memory: Option<u64>,
    pub tasks: Option<u64>,
    pub restarts: Option<u64>,
    pub exit_status: Option<i64>,
    /// 「Tue 2026-09-15 17:49:07 CST」, as the host writes it.
    pub started: Option<String>,
    pub stopped: Option<String>,
    pub path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The host's clock as the tables are read: 2026-11-19 08:00:00 UTC.
    const NOW: i64 = 1_795_075_200;
    const HOUR: i64 = 3600;

    fn services() -> Vec<Service> {
        vec![
            Service::new(
                "socialc.service",
                "Social Callback Proxy Service",
                "active",
                "enabled",
                "/etc/systemd/system/socialc.service",
            ),
            Service::new(
                "ssh.service",
                "OpenBSD Secure Shell server",
                "active",
                "enabled",
                "/lib/systemd/system/ssh.service",
            ),
            Service::new(
                "apport.service",
                "LSB: automatic crash report generation",
                "active",
                "generated",
                "/run/systemd/generator.late/apport.service",
            ),
            Service::new(
                "ModemManager.service",
                "Modem Manager",
                "inactive",
                "disabled",
                "/lib/systemd/system/ModemManager.service",
            ),
            Service::new(
                "certbot.service",
                "Certbot",
                "failed",
                "static",
                "/etc/systemd/system/certbot.service",
            ),
            // Read twice under two names, once as itself.
            Service::new(
                "ssh.service",
                "OpenBSD Secure Shell server",
                "active",
                "enabled",
                "/lib/systemd/system/ssh.service",
            ),
        ]
    }

    /// A timer waiting to start `unit`, at no time known yet.
    fn timer(name: &str, unit: &str, path: &str) -> Timer {
        Timer {
            name: name.into(),
            description: String::new(),
            active: ActiveState::Active,
            sub: "waiting".into(),
            file_state: "enabled".into(),
            custom: custom_unit(path),
            path: path.into(),
            unit: unit.into(),
            triggers: Vec::new(),
            next: None,
            last: None,
            unit_state: None,
        }
    }

    /// logrotate's timer starts a service the list does not have,
    /// certbot's one that failed, and the administrator's backup timer is
    /// stopped.
    fn timers() -> Vec<Timer> {
        vec![
            Timer {
                next: Some(NOW + 16 * HOUR),
                last: Some(NOW - 8 * HOUR),
                triggers: vec![Trigger::Calendar("*-*-* 00:00:00".into())],
                ..timer(
                    "logrotate.timer",
                    "logrotate.service",
                    "/lib/systemd/system/logrotate.timer",
                )
            },
            Timer {
                active: ActiveState::Inactive,
                sub: "dead".into(),
                file_state: "disabled".into(),
                last: Some(NOW - 50 * HOUR),
                ..timer(
                    "backup.timer",
                    "backup.service",
                    "/etc/systemd/system/backup.timer",
                )
            },
            Timer {
                next: Some(NOW + 3 * HOUR),
                last: Some(NOW - 9 * HOUR),
                ..timer(
                    "certbot.timer",
                    "certbot.service",
                    "/lib/systemd/system/certbot.timer",
                )
            },
        ]
    }

    fn table() -> ServiceTable {
        ServiceTable::new(
            services(),
            timers(),
            Some("systemd 249".into()),
            Some("degraded".into()),
            Some(NOW),
        )
    }

    fn names(table: &ServiceTable, rows: &[ServiceRow]) -> Vec<String> {
        rows.iter()
            .map(|row| match *row {
                ServiceRow::Group {
                    kind,
                    custom,
                    count,
                } => {
                    let group = match (kind, custom) {
                        (UnitKind::Service, true) => "自定义服务",
                        (UnitKind::Service, false) => "系统服务",
                        (UnitKind::Timer, true) => "自定义定时器",
                        (UnitKind::Timer, false) => "系统定时器",
                    };
                    format!("{group} {count}")
                }
                ServiceRow::Service(index) => table.services()[index].name.clone(),
                ServiceRow::Timer(index) => table.timers()[index].name.clone(),
            })
            .collect()
    }

    #[test]
    fn the_administrators_services_come_first_under_each_tab() {
        let table = table();
        assert_eq!(table.summary(), "systemd 249 · 降级运行");
        assert!(table.degraded());
        assert_eq!(table.counts(UnitKind::Service, ""), [5, 3, 1, 1]);
        let rows = |filter| names(&table, &table.rows(UnitKind::Service, filter, ""));
        assert_eq!(
            rows(ServiceFilter::All),
            [
                "自定义服务 2",
                "certbot.service",
                "socialc.service",
                "系统服务 3",
                "ModemManager.service",
                "apport.service",
                "ssh.service",
            ]
        );
        assert_eq!(
            rows(ServiceFilter::Running),
            [
                "自定义服务 1",
                "socialc.service",
                "系统服务 2",
                "apport.service",
                "ssh.service",
            ]
        );
        assert_eq!(
            rows(ServiceFilter::Failed),
            ["自定义服务 1", "certbot.service"]
        );
    }

    #[test]
    fn the_search_finds_names_and_descriptions() {
        let table = table();
        let rows = |query| {
            names(
                &table,
                &table.rows(UnitKind::Service, ServiceFilter::All, query),
            )
        };
        assert_eq!(rows(" SECURE "), ["系统服务 1", "ssh.service"]);
        assert_eq!(rows("modem"), ["系统服务 1", "ModemManager.service"]);
        assert_eq!(table.counts(UnitKind::Service, "cert"), [1, 0, 0, 1]);
        assert!(
            table
                .rows(UnitKind::Service, ServiceFilter::Running, "cert")
                .is_empty()
        );
    }

    #[test]
    fn a_service_offers_what_its_state_allows() {
        let table = table();
        let service = |name: &str| table.service(name).expect(name);
        let ssh = service("ssh.service");
        assert_eq!(
            ssh.commands(),
            [ServiceCommand::Stop, ServiceCommand::Restart]
        );
        assert_eq!(ssh.boot_command(), Some(ServiceCommand::Disable));
        let modem = service("ModemManager.service");
        assert_eq!(modem.commands(), [ServiceCommand::Start]);
        assert_eq!(modem.boot_command(), Some(ServiceCommand::Enable));
        // Neither a static unit nor a generated one can be enabled.
        assert_eq!(service("certbot.service").boot_command(), None);
        assert_eq!(service("apport.service").boot_command(), None);
        assert_eq!(
            service("certbot.service").commands(),
            [ServiceCommand::Start]
        );
    }

    #[test]
    fn timers_come_by_when_they_go_off_and_a_failed_job_is_failed() {
        let table = table();
        let rows = |filter| names(&table, &table.rows(UnitKind::Timer, filter, ""));
        assert_eq!(
            rows(ServiceFilter::All),
            [
                "自定义定时器 1",
                "backup.timer",
                "系统定时器 2",
                "certbot.timer",
                "logrotate.timer",
            ]
        );
        // certbot's timer waits, but what it started last failed.
        assert_eq!(table.counts(UnitKind::Timer, ""), [3, 1, 1, 1]);
        assert_eq!(
            rows(ServiceFilter::Failed),
            ["系统定时器 1", "certbot.timer"]
        );
        assert_eq!(
            rows(ServiceFilter::Running),
            ["系统定时器 1", "logrotate.timer"]
        );

        let certbot = table.timer("certbot.timer").expect("certbot");
        assert!(certbot.failed_last());
        assert_eq!(certbot.state_label(), "等待中");
        assert_eq!(table.timer("logrotate.timer").unwrap().unit_state, None);
        assert_eq!(
            table.service("certbot.service").unwrap().timers,
            ["certbot.timer"]
        );
        assert!(table.service("ssh.service").unwrap().timers.is_empty());

        // A timer is found by what it starts, too.
        assert_eq!(
            names(
                &table,
                &table.rows(UnitKind::Timer, ServiceFilter::All, "LOGROTATE.SERVICE")
            ),
            ["系统定时器 1", "logrotate.timer"]
        );
    }

    #[test]
    fn a_timer_offers_what_its_state_allows() {
        let table = table();
        let logrotate = table.timer("logrotate.timer").unwrap();
        assert_eq!(logrotate.commands(), [ServiceCommand::Stop]);
        assert_eq!(logrotate.boot_command(), Some(ServiceCommand::Disable));
        assert!(logrotate.runs_service());
        let backup = table.timer("backup.timer").unwrap();
        assert_eq!(backup.commands(), [ServiceCommand::Start]);
        assert_eq!(backup.boot_command(), Some(ServiceCommand::Enable));
        assert_eq!(backup.state_label(), "已停止");
        let elapsed = Timer {
            sub: "elapsed".into(),
            unit: "shutdown.target".into(),
            ..logrotate.clone()
        };
        assert_eq!(elapsed.state_label(), "已触发");
        assert!(!elapsed.runs_service());
        assert_eq!(ServiceCommand::Run.systemctl(), "start --no-block");
        assert_eq!(ServiceCommand::Run.label(), "立即运行");
    }

    #[test]
    fn a_timer_says_when_it_goes_off_and_went_off() {
        let table = table();
        let times = |name: &str, now: i64| table.timer(name).unwrap().times(now);
        assert_eq!(
            times("logrotate.timer", NOW).as_deref(),
            Some("下次 16 小时后 · 上次 8 小时前")
        );
        assert_eq!(times("backup.timer", NOW).as_deref(), Some("上次 2 天前"));
        // Its time came after the list was read.
        assert_eq!(
            times("certbot.timer", NOW + 3 * HOUR).as_deref(),
            Some("已到下次运行时间 · 上次 12 小时前")
        );
        let never = timer("never.timer", "never.service", "");
        assert_eq!(never.times(NOW), None);
        assert!(format_moment(NOW + 16 * HOUR, NOW).ends_with("（16 小时后）"));
        assert_eq!(
            Trigger::After(TriggerBase::Boot, "15min".into()).label(),
            "开机 15min 后"
        );
        assert_eq!(
            Trigger::Calendar("*-*-* 00:00:00".into()).label(),
            "*-*-* 00:00:00"
        );
    }

    #[test]
    fn times_read_as_how_far_off_they_are() {
        assert_eq!(format_from_now(NOW + 30, NOW), "1 分钟内");
        assert_eq!(format_from_now(NOW + 5 * 60 + 59, NOW), "5 分钟后");
        assert_eq!(format_from_now(NOW + 3 * HOUR, NOW), "3 小时后");
        assert_eq!(format_from_now(NOW + 400 * 24 * HOUR, NOW), "400 天后");
        assert_eq!(format_from_now(NOW, NOW), "刚刚");
        assert_eq!(format_from_now(NOW - 59, NOW), "刚刚");
        assert_eq!(format_from_now(NOW - 2 * 60, NOW), "2 分钟前");
        assert_eq!(format_from_now(NOW - 23 * HOUR, NOW), "23 小时前");
        assert_eq!(format_from_now(NOW - 48 * HOUR, NOW), "2 天前");
    }

    #[test]
    fn english_says_how_far_off_a_time_is() {
        crate::i18n::isolate_thread();
        crate::i18n::set_locale("en");
        assert_eq!(format_from_now(NOW + 30, NOW), "within a minute");
        assert_eq!(format_from_now(NOW + 60, NOW), "in 1 minute");
        assert_eq!(format_from_now(NOW + 2 * HOUR, NOW), "in 2 hours");
        assert_eq!(format_from_now(NOW - 24 * HOUR, NOW), "1 day ago");
        let table = table();
        assert_eq!(
            table
                .timer("logrotate.timer")
                .unwrap()
                .times(NOW)
                .as_deref(),
            Some("Next in 16 hours · Ran 8 hours ago")
        );
    }

    #[test]
    fn the_host_clock_runs_on_from_what_it_said() {
        let clock = HostClock::new(Some(NOW));
        assert!((NOW..NOW + 5).contains(&clock.now()));
        let here = chrono::Utc::now().timestamp();
        assert!((here..here + 5).contains(&HostClock::new(None).now()));
    }

    #[test]
    fn states_read_in_chinese() {
        assert_eq!(sub_state_label("exited"), "已退出");
        assert_eq!(sub_state_label("stop-sigterm"), "正在停止");
        assert_eq!(sub_state_label("mounted"), "mounted");
        assert_eq!(file_state_label("generated"), Some("自动生成".into()));
        assert_eq!(file_state_label(""), None);
        assert_eq!(load_state_label("loaded"), "已加载");
        assert_eq!(system_state_label("running"), "运行正常");
    }
}
