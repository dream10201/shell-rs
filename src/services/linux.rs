//! Reading and running a Linux host's systemd services and timers: the
//! commands, and the parsers for what they print.

use std::collections::HashMap;

use super::model::{
    ActiveState, Service, ServiceCommand, ServiceStatus, ServiceTable, Timer, Trigger, TriggerBase,
    custom_unit,
};
use crate::i18n::t;

/// What the list asks `systemctl show` for each service and timer; those
/// a unit has not are left out of what it says.
const LIST_PROPERTIES: [&str; 13] = [
    "Id",
    "Description",
    "ActiveState",
    "SubState",
    "UnitFileState",
    "FragmentPath",
    "LoadState",
    // A timer's.
    "Unit",
    "TimersCalendar",
    "TimersMonotonic",
    "NextElapseUSecRealtime",
    "NextElapseUSecMonotonic",
    "LastTriggerUSec",
];

/// What a service's details ask for.
const STATUS_PROPERTIES: [&str; 14] = [
    "Id",
    "Description",
    "LoadState",
    "ActiveState",
    "SubState",
    "UnitFileState",
    "FragmentPath",
    "MainPID",
    "MemoryCurrent",
    "TasksCurrent",
    "NRestarts",
    "ExecMainStatus",
    "ActiveEnterTimestamp",
    "InactiveEnterTimestamp",
];

/// `-p Id -p Description …`: one property a flag, which systemd before 230
/// needs (later ones take them comma-separated too).
fn properties(names: &[&str]) -> String {
    names
        .iter()
        .map(|name| format!("-p {name}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// What 系统服务 runs on the host for the list: systemd's version and how
/// the machine is doing, the host's clock and how long it has been up,
/// then every service's and timer's state in one `systemctl show`: those
/// with a unit file (but templates, which run only as instances) and
/// those loaded without one (instances, and some LSB scripts). Each part
/// under an `@@` line of its own.
///
/// `systemctl` writes its times in UTC (`TZ`), so that they say which
/// moment they are; the clock and the uptime put a timer's time since boot
/// on the clock.
///
/// One line, in single quotes for `sh -c`, so that whatever the login shell
/// is (bash, zsh, fish, csh) it hands the script to `sh` untouched: the
/// script has no single quote, no `!` for csh to expand, and no two
/// backslashes in a row for fish to make one. The `case` patterns open
/// with `(`: inside `$(…)`, bash 3.2 takes a pattern's bare `)` for the
/// end of the substitution.
pub fn command() -> String {
    format!(
        "sh -c 'export LC_ALL=C TZ=UTC0 PATH=$PATH:/usr/sbin:/sbin; \
         if test -d /run/systemd/system; then \
         echo @@version; systemctl --version 2>/dev/null; \
         echo @@state; systemctl is-system-running 2>/dev/null; \
         echo @@now; date +%s; \
         echo @@uptime; {{ read -r up rest < /proc/uptime; }} 2>/dev/null; echo \"$up\"; \
         names=$({{ systemctl list-unit-files --type=service,timer --no-legend --no-pager 2>/dev/null; \
         systemctl list-units --type=service,timer --all --no-legend --no-pager --plain 2>/dev/null; }} \
         | while read -r name rest; do case $name in (*@.service|*@.timer) ;; (*.service|*.timer) printf \"%s\\n\" \"$name\";; esac; done); \
         echo @@units; systemctl show {properties} -- $names 2>/dev/null; \
         else echo @@unsupported; uname -s; fi'",
        properties = properties(&LIST_PROPERTIES),
    )
}

/// Whether `name` is a service's or a timer's name and nothing else, so
/// that it can go into a command: systemd allows letters, digits and
/// `:_.@-`, and `\` for its escapes (「\x2d」).
pub fn valid_name(name: &str) -> bool {
    (name.ends_with(".service") || name.ends_with(".timer"))
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ":_.@-\\".contains(character))
        && !name.contains("\\\\")
}

/// `script` as root: as it is when the login is root, else through `sudo`
/// without a password, which fails at once where one is needed.
fn as_root(script: &str) -> String {
    format!("if test \"$(id -u)\" = 0; then {script}; else sudo -n {script}; fi")
}

/// What runs `command` on the service or timer `name`, saying how it went
/// in an `@@status` line with `systemctl`'s complaint before it. `None`
/// for a name that is not one.
pub fn control_command(name: &str, command: ServiceCommand) -> Option<String> {
    valid_name(name).then(|| {
        let script = format!("systemctl {} -- \"{name}\"", command.systemctl());
        format!("sh -c '{} 2>&1; echo @@status $?'", as_root(&script))
    })
}

/// How a command went, from what `control_command` printed: why not, when
/// it did not.
pub fn controlled(output: &str) -> Result<(), String> {
    let (said, status) = output
        .rsplit_once("@@status")
        .ok_or_else(|| t!("tools.no_answer").to_string())?;
    if status.trim() == "0" {
        return Ok(());
    }
    let said = said.trim();
    Err(
        if said.contains("password is required")
            || said.contains("Interactive authentication required")
            || said.contains("Access denied")
            || said.contains("not in the sudoers")
        {
            t!("services.error.needs_root").into()
        } else if said.contains("command not found") && said.contains("sudo") {
            t!("services.error.needs_root_no_sudo").into()
        } else if said.is_empty() {
            t!("services.error.status", status = status.trim()).into()
        } else {
            said.lines().next().unwrap_or(said).trim().to_owned()
        },
    )
}

/// What reads a service's state in full, for its details.
pub fn status_command(name: &str) -> Option<String> {
    valid_name(name).then(|| {
        format!(
            "sh -c 'export LC_ALL=C; systemctl show {} -- \"{name}\"'",
            properties(&STATUS_PROPERTIES)
        )
    })
}

/// The last 200 lines of journal of `units` together, the newest last: a
/// service's, or a timer's and the service it starts. The journal is
/// root's to read, besides some groups', so a login that is not root asks
/// `sudo` first and settles for what it may see.
pub fn log_command(units: &[&str]) -> Option<String> {
    (!units.is_empty() && units.iter().all(|unit| valid_name(unit))).then(|| {
        let units = units
            .iter()
            .map(|unit| format!("-u \"{unit}\""))
            .collect::<Vec<_>>()
            .join(" ");
        let journal = format!("journalctl {units} -n 200 --no-pager");
        format!(
            "sh -c 'export LC_ALL=C; if test \"$(id -u)\" = 0; then {journal}; \
             else sudo -n {journal} 2>/dev/null || {journal}; fi 2>&1'"
        )
    })
}

/// What the list's command printed.
#[derive(Debug, PartialEq)]
pub enum Parsed {
    Table(ServiceTable),
    /// Not a host whose services systemd runs. Holds `uname -s` when the
    /// host has one: 「Linux」 without systemd, 「Darwin」.
    Unsupported(Option<String>),
}

pub fn parse(output: &str) -> Parsed {
    let mut sections: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut current = None;
    for line in output.lines() {
        if let Some(name) = line.strip_prefix("@@") {
            current = Some(name.trim());
            sections.entry(name.trim()).or_default();
        } else if let Some(name) = current {
            sections.entry(name).or_default().push(line);
        }
    }
    let section = |name: &str| sections.get(name).map(Vec::as_slice).unwrap_or_default();
    let first = |name: &str| {
        section(name)
            .iter()
            .map(|line| line.trim())
            .find(|line| !line.is_empty())
    };
    if !sections.contains_key("units") {
        // A shell other than sh's (cmd.exe) prints nothing we know.
        return Parsed::Unsupported(first("unsupported").map(str::to_owned));
    }
    // 「systemd 249 (249.11-0ubuntu3.12)」.
    let version = first("version").map(|line| {
        line.split_whitespace()
            .take(2)
            .collect::<Vec<_>>()
            .join(" ")
    });
    let now = first("now").and_then(|line| line.parse::<i64>().ok());
    // 「86400.52 170000.11」: up, and idle.
    let uptime =
        first("uptime").and_then(|line| line.split_whitespace().next()?.parse::<f64>().ok());
    let mut services = Vec::new();
    let mut timers = Vec::new();
    for properties in blocks(section("units")) {
        let name = properties.get("Id");
        // A unit no file is found for is only worth a line when it
        // failed.
        if properties.get("LoadState") == "not-found" && properties.get("ActiveState") != "failed" {
            continue;
        }
        if name.ends_with(".service") {
            services.push(Service::new(
                name,
                properties.get("Description"),
                properties.get("ActiveState"),
                properties.get("UnitFileState"),
                properties.get("FragmentPath"),
            ));
        } else if name.ends_with(".timer") {
            timers.push(timer(&properties, now, uptime));
        }
    }
    Parsed::Table(ServiceTable::new(
        services,
        timers,
        version,
        first("state").map(str::to_owned),
        now,
    ))
}

/// A timer from what `systemctl show` says of it, its times put on the
/// host's clock, which said `now` and had been up `uptime` seconds.
fn timer(properties: &Properties, now: Option<i64>, uptime: Option<f64>) -> Timer {
    let path = properties.get("FragmentPath");
    // A time since boot: `OnBootSec=`, `OnUnitActiveSec=`.
    let since_boot = span(properties.get("NextElapseUSecMonotonic"))
        .zip(now.zip(uptime))
        .map(|(at, (now, uptime))| now + (at - uptime) as i64);
    let next = match (moment(properties.get("NextElapseUSecRealtime")), since_boot) {
        (Some(calendar), Some(since_boot)) => Some(calendar.min(since_boot)),
        (calendar, since_boot) => calendar.or(since_boot),
    };
    let triggers = properties
        .all("TimersCalendar")
        .filter_map(timer_setting)
        .map(|(_, spec)| Trigger::Calendar(spec.to_owned()))
        .chain(
            properties
                .all("TimersMonotonic")
                .filter_map(timer_setting)
                .filter_map(|(base, span)| {
                    Some(Trigger::After(trigger_base(base)?, span.to_owned()))
                }),
        )
        .collect();
    Timer {
        name: properties.get("Id").to_owned(),
        description: properties.get("Description").to_owned(),
        active: ActiveState::parse(properties.get("ActiveState")),
        sub: properties.get("SubState").to_owned(),
        file_state: properties.get("UnitFileState").to_owned(),
        custom: custom_unit(path),
        path: path.to_owned(),
        unit: properties.get("Unit").to_owned(),
        triggers,
        next,
        last: moment(properties.get("LastTriggerUSec")),
        unit_state: None,
    }
}

/// One of a timer's settings as `systemctl show` gives them, 「{
/// OnCalendar=*-*-* 00:00:00 ; next_elapse=… }」: 「OnCalendar」 and
/// 「*-*-* 00:00:00」.
fn timer_setting(value: &str) -> Option<(&str, &str)> {
    let inside = value.trim().strip_prefix('{')?.strip_suffix('}')?;
    let setting = inside.split(" ; ").next()?;
    let (name, value) = setting.split_once('=')?;
    Some((name.trim(), value.trim()))
}

/// What a time span counts from, by systemd's name for the setting:
/// 「OnBootUSec」.
fn trigger_base(name: &str) -> Option<TriggerBase> {
    let name = name
        .strip_suffix("USec")
        .or_else(|| name.strip_suffix("Sec"))
        .unwrap_or(name);
    Some(match name {
        "OnActive" => TriggerBase::Active,
        "OnBoot" => TriggerBase::Boot,
        "OnStartup" => TriggerBase::Startup,
        "OnUnitActive" => TriggerBase::UnitActive,
        "OnUnitInactive" => TriggerBase::UnitInactive,
        _ => return None,
    })
}

/// A moment `systemctl show` gives, in seconds since the epoch: a
/// timestamp, in UTC as the list's script has it (「Thu 2026-11-20 00:00:00
/// UTC」), or, from a systemd that wrote a timer's moments as time spans,
/// the span since the epoch (「56y 10month 3w …」). `None` for none:
/// nothing, 「n/a」, 0.
fn moment(value: &str) -> Option<i64> {
    let words = value.split_whitespace().collect::<Vec<_>>();
    let stamp = words.windows(2).find_map(|pair| {
        chrono::NaiveDateTime::parse_from_str(&pair.join(" "), "%Y-%m-%d %H:%M:%S").ok()
    });
    match stamp {
        Some(stamp) => Some(stamp.and_utc().timestamp()),
        None => span(value).map(|seconds| seconds as i64),
    }
    .filter(|seconds| *seconds > 0)
}

/// A time span as systemd writes one, in seconds: 「1d 15min」, 「1min
/// 30.500s」. `None` for anything else: nothing, 「infinity」, 「n/a」,
/// and 0, which systemd writes without a unit.
fn span(value: &str) -> Option<f64> {
    let mut seconds = 0.;
    let mut any = false;
    for word in value.split_whitespace() {
        let unit_at = word.find(|c: char| !c.is_ascii_digit() && c != '.')?;
        let (number, unit) = word.split_at(unit_at);
        let unit = match unit {
            "y" => 31_557_600.,
            "month" => 2_629_800.,
            "w" => 604_800.,
            "d" => 86_400.,
            "h" => 3600.,
            "min" => 60.,
            "s" => 1.,
            "ms" => 1e-3,
            "us" | "μs" | "µs" => 1e-6,
            _ => return None,
        };
        seconds += number.parse::<f64>().ok()? * unit;
        any = true;
    }
    any.then_some(seconds)
}

/// One unit's `Key=value` lines from `systemctl show`, in order: an array
/// gives a line for each of its values.
struct Properties<'a>(Vec<(&'a str, &'a str)>);

impl<'a> Properties<'a> {
    /// The value of `name`; empty when there is none.
    fn get(&self, name: &str) -> &'a str {
        self.all(name).last().unwrap_or_default()
    }

    /// Every value of `name`.
    fn all(&self, name: &str) -> impl Iterator<Item = &'a str> {
        self.0
            .iter()
            .filter(move |(key, _)| *key == name)
            .map(|(_, value)| *value)
    }
}

/// `systemctl show`'s `Key=value` lines, a blank line between units.
fn blocks<'a>(lines: &[&'a str]) -> Vec<Properties<'a>> {
    let mut blocks = vec![Properties(Vec::new())];
    for line in lines {
        match line.split_once('=') {
            Some((key, value)) => {
                blocks
                    .last_mut()
                    .expect("one at least")
                    .0
                    .push((key.trim(), value.trim()));
            }
            None if line.trim().is_empty() => blocks.push(Properties(Vec::new())),
            None => {}
        }
    }
    blocks.retain(|block| !block.0.is_empty());
    blocks
}

/// A service's details from what `status_command` printed.
pub fn parse_status(output: &str) -> Option<ServiceStatus> {
    let lines: Vec<&str> = output.lines().collect();
    let properties = blocks(&lines).into_iter().next()?;
    let property = |name: &str| {
        Some(properties.get(name)).filter(|value| !value.is_empty() && *value != "[not set]")
    };
    // systemd writes an unset number as the largest one it has.
    let number = |name: &str| {
        property(name)
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value != u64::MAX)
    };
    Some(ServiceStatus {
        description: property("Description").unwrap_or_default().to_owned(),
        load: property("LoadState").unwrap_or_default().to_owned(),
        active: property("ActiveState").unwrap_or_default().to_owned(),
        sub: property("SubState").unwrap_or_default().to_owned(),
        file_state: property("UnitFileState").unwrap_or_default().to_owned(),
        main_pid: number("MainPID")
            .filter(|pid| *pid != 0)
            .map(|pid| pid as u32),
        memory: number("MemoryCurrent"),
        tasks: number("TasksCurrent"),
        restarts: number("NRestarts"),
        exit_status: property("ExecMainStatus").and_then(|value| value.parse().ok()),
        started: property("ActiveEnterTimestamp").map(str::to_owned),
        stopped: property("InactiveEnterTimestamp").map(str::to_owned),
        path: property("FragmentPath").map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::model::{ServiceFilter, UnitKind};

    const LIST: &str = "\
@@version
systemd 249 (249.11-0ubuntu3.12)
+PAM +AUDIT +SELINUX
@@state
degraded
@@units
Id=socialc.service
Description=Social Callback Proxy Service
ActiveState=active
UnitFileState=enabled
FragmentPath=/etc/systemd/system/socialc.service
LoadState=loaded

Id=apport.service
Description=LSB: automatic crash report generation
ActiveState=active
UnitFileState=generated
FragmentPath=/run/systemd/generator.late/apport.service
LoadState=loaded

Id=auditd.service
Description=auditd.service
ActiveState=inactive
UnitFileState=
FragmentPath=
LoadState=not-found

Id=certbot.service
Description=Certbot
ActiveState=failed
UnitFileState=static
FragmentPath=/lib/systemd/system/certbot.service
LoadState=loaded
";

    /// 2026-11-19 08:00:00 UTC, by the host's clock, a day after it
    /// booted.
    const NOW: i64 = 1_795_075_200;
    const HOUR: i64 = 3600;

    /// Timers on a calendar and on a time since boot, one written by a
    /// systemd that wrote its moments as time spans, and one stopped.
    const TIMERS: &str = "\
@@version
systemd 249 (249.11-0ubuntu3.12)
@@state
running
@@now
1795075200
@@uptime
86400.00 170000.11
@@units
Id=logrotate.service
Description=Rotate log files
ActiveState=inactive
SubState=dead
UnitFileState=static
FragmentPath=/lib/systemd/system/logrotate.service
LoadState=loaded

Id=logrotate.timer
Description=Daily rotation of log files
ActiveState=active
SubState=waiting
UnitFileState=enabled
FragmentPath=/lib/systemd/system/logrotate.timer
LoadState=loaded
Unit=logrotate.service
TimersCalendar={ OnCalendar=*-*-* 00:00:00 ; next_elapse=Fri 2026-11-20 00:00:00 UTC }
NextElapseUSecRealtime=Fri 2026-11-20 00:00:00 UTC
NextElapseUSecMonotonic=infinity
LastTriggerUSec=Thu 2026-11-19 00:00:00 UTC

Id=systemd-tmpfiles-clean.timer
Description=Daily Cleanup of Temporary Directories
ActiveState=active
SubState=waiting
UnitFileState=static
FragmentPath=/lib/systemd/system/systemd-tmpfiles-clean.timer
LoadState=loaded
Unit=systemd-tmpfiles-clean.service
TimersMonotonic={ OnBootUSec=15min ; next_elapse=15min }
TimersMonotonic={ OnUnitActiveUSec=1d ; next_elapse=1d 15min }
NextElapseUSecRealtime=n/a
NextElapseUSecMonotonic=1d 15min
LastTriggerUSec=Wed 2026-11-18 08:15:00 UTC

Id=old.timer
Description=Written by an old systemd
ActiveState=active
SubState=waiting
UnitFileState=enabled
FragmentPath=/etc/systemd/system/old.timer
LoadState=loaded
Unit=old.service
TimersCalendar={ OnCalendar=*-*-* 09:00:00 ; next_elapse=Thu 2026-11-19 09:00:00 UTC }
NextElapseUSecRealtime=56y 10month 2w 4d
NextElapseUSecMonotonic=0
LastTriggerUSec=56y 10month 2w 3d 22h

Id=fstrim.timer
Description=Discard unused blocks once a week
ActiveState=inactive
SubState=dead
UnitFileState=disabled
FragmentPath=/lib/systemd/system/fstrim.timer
LoadState=loaded
Unit=fstrim.service
TimersCalendar={ OnCalendar=Mon *-*-* 00:00:00 ; next_elapse=n/a }
NextElapseUSecRealtime=
NextElapseUSecMonotonic=0
LastTriggerUSec=n/a
";

    #[test]
    fn systemctl_show_gives_the_services_but_those_without_a_file() {
        let Parsed::Table(table) = parse(LIST) else {
            panic!("not a table");
        };
        assert_eq!(table.summary(), "systemd 249 · 降级运行");
        let services: Vec<(&str, ActiveState, &str, bool)> = table
            .services()
            .iter()
            .map(|service| {
                (
                    service.name.as_str(),
                    service.active,
                    service.file_state.as_str(),
                    service.custom,
                )
            })
            .collect();
        assert_eq!(
            services,
            [
                ("apport.service", ActiveState::Active, "generated", false),
                ("certbot.service", ActiveState::Failed, "static", false),
                ("socialc.service", ActiveState::Active, "enabled", true),
            ]
        );
        assert_eq!(table.counts(UnitKind::Service, ""), [3, 2, 0, 1]);
        assert_eq!(
            table.rows(UnitKind::Service, ServiceFilter::All, "").len(),
            5
        );
        assert!(table.timers().is_empty());
        assert_eq!(table.now(), None);
    }

    #[test]
    fn systemctl_show_gives_the_timers_on_the_hosts_clock() {
        let Parsed::Table(table) = parse(TIMERS) else {
            panic!("not a table");
        };
        assert_eq!(table.now(), Some(NOW));
        let timers: Vec<(&str, Option<i64>, Option<i64>)> = table
            .timers()
            .iter()
            .map(|timer| (timer.name.as_str(), timer.next, timer.last))
            .collect();
        assert_eq!(
            timers,
            [
                // Fifteen minutes on from a day since boot.
                (
                    "systemd-tmpfiles-clean.timer",
                    Some(NOW + 900),
                    Some(NOW - 23 * HOUR - 45 * 60)
                ),
                ("old.timer", Some(NOW + HOUR), Some(NOW - HOUR)),
                (
                    "logrotate.timer",
                    Some(NOW + 16 * HOUR),
                    Some(NOW - 8 * HOUR)
                ),
                ("fstrim.timer", None, None),
            ]
        );
        let timer = |name: &str| table.timer(name).expect(name);
        let logrotate = timer("logrotate.timer");
        assert_eq!(
            logrotate.triggers,
            [Trigger::Calendar("*-*-* 00:00:00".into())]
        );
        assert_eq!(logrotate.unit, "logrotate.service");
        assert_eq!(logrotate.unit_state, Some(ActiveState::Inactive));
        assert_eq!(logrotate.path, "/lib/systemd/system/logrotate.timer");
        assert!(!logrotate.custom);
        assert_eq!(
            timer("systemd-tmpfiles-clean.timer").triggers,
            [
                Trigger::After(TriggerBase::Boot, "15min".into()),
                Trigger::After(TriggerBase::UnitActive, "1d".into()),
            ]
        );
        assert_eq!(timer("systemd-tmpfiles-clean.timer").unit_state, None);
        assert!(timer("old.timer").custom);
        assert_eq!(timer("fstrim.timer").active, ActiveState::Inactive);
        assert_eq!(
            table.service("logrotate.service").unwrap().timers,
            ["logrotate.timer"]
        );
        assert_eq!(table.counts(UnitKind::Timer, ""), [4, 3, 1, 0]);
    }

    #[test]
    fn moments_and_spans_read_as_seconds() {
        assert_eq!(moment("Thu 2026-11-19 08:00:00 UTC"), Some(NOW));
        assert_eq!(moment("56y 10month 2w 4d"), Some(NOW + HOUR));
        for none in ["", "n/a", "0", "infinity"] {
            assert_eq!(moment(none), None, "{none:?}");
        }
        assert_eq!(span("1d 15min"), Some(87_300.));
        assert_eq!(span("1min 30.500s"), Some(90.5));
        assert_eq!(span("500ms"), Some(0.5));
        for none in ["", "0", "infinity", "n/a", "15 minutes"] {
            assert_eq!(span(none), None, "{none:?}");
        }
        assert_eq!(
            timer_setting("{ OnBootUSec=15min ; next_elapse=15min }"),
            Some(("OnBootUSec", "15min"))
        );
        assert_eq!(timer_setting("OnBootUSec=15min"), None);
        assert_eq!(
            trigger_base("OnUnitInactiveUSec"),
            Some(TriggerBase::UnitInactive)
        );
        assert_eq!(trigger_base("OnClockChange"), None);
    }

    #[test]
    fn a_host_without_systemd_says_what_it_is() {
        assert_eq!(
            parse("@@unsupported\nLinux\n"),
            Parsed::Unsupported(Some("Linux".into()))
        );
        assert_eq!(parse(""), Parsed::Unsupported(None));
    }

    #[test]
    fn details_leave_out_what_systemd_has_not_set() {
        let status = parse_status(
            "Id=socialc.service\nDescription=Social Callback Proxy Service\nLoadState=loaded\n\
             ActiveState=active\nSubState=running\nUnitFileState=enabled\n\
             FragmentPath=/etc/systemd/system/socialc.service\nMainPID=781\n\
             MemoryCurrent=1990656\nTasksCurrent=5\nNRestarts=0\nExecMainStatus=0\n\
             ActiveEnterTimestamp=Tue 2026-09-15 17:49:07 CST\nInactiveEnterTimestamp=\n",
        )
        .expect("details");
        assert_eq!(status.main_pid, Some(781));
        assert_eq!(status.memory, Some(1_990_656));
        assert_eq!(status.tasks, Some(5));
        assert_eq!(
            status.started.as_deref(),
            Some("Tue 2026-09-15 17:49:07 CST")
        );
        assert_eq!(status.stopped, None);

        let stopped = parse_status(
            "Id=a.service\nMainPID=0\nMemoryCurrent=[not set]\nTasksCurrent=18446744073709551615\n",
        )
        .expect("details");
        assert_eq!(
            (stopped.main_pid, stopped.memory, stopped.tasks),
            (None, None, None)
        );
    }

    #[test]
    fn only_a_services_or_timers_name_goes_into_a_command() {
        assert!(valid_name("nginx.service"));
        assert!(valid_name("getty@tty1.service"));
        assert!(valid_name("systemd-fsck@dev-disk-by\\x2duuid-1234.service"));
        assert!(valid_name("logrotate.timer"));
        assert!(!valid_name("nginx"));
        assert!(!valid_name("shutdown.target"));
        assert!(!valid_name("a;reboot.service"));
        assert!(!valid_name("a b.service"));
        assert!(!valid_name("$(reboot).service"));
        assert_eq!(control_command("a\"b.service", ServiceCommand::Stop), None);
        assert_eq!(
            control_command("nginx.service", ServiceCommand::Restart).as_deref(),
            Some(
                "sh -c 'if test \"$(id -u)\" = 0; then systemctl restart -- \"nginx.service\"; \
                 else sudo -n systemctl restart -- \"nginx.service\"; fi 2>&1; echo @@status $?'"
            )
        );
        // A timer's job is started without waiting for it to finish.
        assert!(
            control_command("logrotate.service", ServiceCommand::Run)
                .unwrap()
                .contains("then systemctl start --no-block -- \"logrotate.service\";")
        );
        assert!(
            log_command(&["logrotate.timer", "logrotate.service"])
                .unwrap()
                .contains(
                    "journalctl -u \"logrotate.timer\" -u \"logrotate.service\" -n 200 --no-pager"
                )
        );
        assert_eq!(log_command(&[]), None);
        assert_eq!(log_command(&["a.timer", "a;b.service"]), None);
    }

    #[test]
    fn a_command_says_why_it_did_not_run() {
        assert_eq!(controlled("@@status 0\n"), Ok(()));
        assert_eq!(
            controlled("sudo: a password is required\n@@status 1\n"),
            Err("需要 root 权限：以 root 登录，或给这个用户配置免密码的 sudo".into())
        );
        assert_eq!(
            controlled(
                "Job for nginx.service failed because the control process exited with error code.\n\
                 See \"systemctl status nginx.service\" and \"journalctl -xeu nginx.service\" for details.\n\
                 @@status 1\n"
            ),
            Err(
                "Job for nginx.service failed because the control process exited with error code."
                    .into()
            )
        );
        assert_eq!(controlled(""), Err("主机没有回答".into()));
    }

    fn scripts() -> Vec<String> {
        vec![
            command(),
            control_command("getty@tty1.service", ServiceCommand::Stop).unwrap(),
            control_command("logrotate.service", ServiceCommand::Run).unwrap(),
            status_command("nginx.service").unwrap(),
            log_command(&["nginx.service"]).unwrap(),
            log_command(&["logrotate.timer", "logrotate.service"]).unwrap(),
        ]
    }

    #[test]
    fn the_commands_survive_any_login_shell() {
        for command in scripts() {
            let script = command
                .strip_prefix("sh -c '")
                .and_then(|rest| rest.strip_suffix('\''))
                .expect("one sh -c in single quotes");
            assert!(!script.contains('\''), "{script}");
            assert!(!script.contains('!'), "{script}");
            assert!(!script.contains('\n'), "{script}");
            assert!(!script.contains("\\\\"), "{script}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_commands_are_valid_sh() {
        for command in scripts() {
            let script = command
                .strip_prefix("sh -c '")
                .and_then(|rest| rest.strip_suffix('\''))
                .unwrap();
            assert!(crate::testing::sh_accepts(script), "{script}");
        }
    }

    /// The list's script, run by this machine's `sh` with a `systemctl`
    /// stood in for, asks `systemctl show` for the right services and
    /// timers, and says what the host's clock says.
    #[cfg(unix)]
    #[test]
    fn the_script_shows_every_service_and_timer_but_templates() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("temp dir");
        let bin = root.path().join("bin");
        let booted = root.path().join("systemd");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&booted).unwrap();
        let systemctl = bin.join("systemctl");
        std::fs::write(
            &systemctl,
            "#!/bin/sh\n\
             case \"$1\" in\n\
             --version) echo 'systemd 249 (249.11)';;\n\
             is-system-running) echo running;;\n\
             list-unit-files) printf '%s\\n' 'ssh.service enabled enabled' 'getty@.service enabled enabled' 'sshd.service alias -' 'logrotate.timer enabled enabled' 'snapshot@.timer disabled enabled';;\n\
             list-units) printf '%s\\n' 'getty@tty1.service loaded active running Getty on tty1' 'ssh.service loaded active running OpenBSD Secure Shell server' 'logrotate.timer loaded active waiting Daily rotation of log files';;\n\
             show) shift; while test \"$1\" = -p; do shift 2; done; shift; for unit in \"$@\"; do \
             id=$unit; test $unit = sshd.service && id=ssh.service; \
             printf 'Id=%s\\nDescription=%s\\nActiveState=active\\nUnitFileState=enabled\\nFragmentPath=/lib/systemd/system/%s\\nLoadState=loaded\\n\\n' $id $id $id; done;;\n\
             esac\n",
        )
        .unwrap();
        std::fs::set_permissions(&systemctl, std::fs::Permissions::from_mode(0o755)).unwrap();

        let command = command().replace("/run/systemd/system", &booted.display().to_string());
        let script = command
            .strip_prefix("sh -c '")
            .and_then(|rest| rest.strip_suffix('\''))
            .unwrap();
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let output = crate::testing::sh(script, Some(&path));
        let Parsed::Table(table) = parse(&String::from_utf8(output.stdout).unwrap()) else {
            panic!("not a table");
        };
        let names: Vec<&str> = table
            .services()
            .iter()
            .map(|service| service.name.as_str())
            .collect();
        // The template is left out, and the alias is ssh itself.
        assert_eq!(names, ["getty@tty1.service", "ssh.service"]);
        let timers: Vec<&str> = table
            .timers()
            .iter()
            .map(|timer| timer.name.as_str())
            .collect();
        assert_eq!(timers, ["logrotate.timer"]);
        assert_eq!(table.summary(), "systemd 249 · 运行正常");
        let here = chrono::Utc::now().timestamp();
        assert!(table.now().is_some_and(|now| (now - here).abs() < 60));
    }
}
