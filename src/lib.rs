use chrono::{Local, NaiveDate, NaiveTime, TimeDelta};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub const DEFAULT_CONFIG: &str = include_str!("../config.example.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub search: SearchConfig,
    pub schedule: ScheduleConfig,
    #[serde(default)]
    pub automation: AutomationConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchConfig {
    #[serde(deserialize_with = "deserialize_time")]
    pub target_time: NaiveTime,
    #[serde(deserialize_with = "deserialize_time")]
    pub start_time: NaiveTime,
    #[serde(deserialize_with = "deserialize_time")]
    pub end_time: NaiveTime,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleConfig {
    #[serde(deserialize_with = "deserialize_time")]
    pub max_end_time: NaiveTime,
    pub reminder_minutes: i64,
    pub rules: Vec<RuleConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuleConfig {
    #[serde(deserialize_with = "deserialize_time")]
    pub until: NaiveTime,
    pub kind: RuleKind,
    pub duration_minutes: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_optional_time")]
    pub end_time: Option<NaiveTime>,
    #[serde(default, deserialize_with = "deserialize_optional_time")]
    pub anchor_start: Option<NaiveTime>,
    #[serde(default, deserialize_with = "deserialize_optional_time")]
    pub anchor_end: Option<NaiveTime>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AutomationConfig {
    #[serde(deserialize_with = "deserialize_time")]
    pub daily_record_time: NaiveTime,
    pub reminder_check_seconds: u64,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            daily_record_time: NaiveTime::from_hms_opt(14, 5, 0).unwrap(),
            reminder_check_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    AddDuration,
    Fixed,
    CarryDelay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DailyRecord {
    pub date: NaiveDate,
    pub wake_time: NaiveTime,
    pub estimated_end_time: NaiveTime,
    pub source: String,
    pub reminder_minutes: i64,
    pub reminder_sent: bool,
    pub reminder_sent_at: Option<String>,
}

fn deserialize_time<'de, D>(deserializer: D) -> Result<NaiveTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    parse_time(&value).map_err(serde::de::Error::custom)
}

fn deserialize_optional_time<'de, D>(deserializer: D) -> Result<Option<NaiveTime>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)?
        .map(|value| parse_time(&value).map_err(serde::de::Error::custom))
        .transpose()
}

pub fn parse_time(value: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(value, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M"))
        .map_err(|_| format!("invalid time `{value}`; expected HH:MM or HH:MM:SS"))
}

pub fn config_path() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("WAKE_CLOCK_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    Ok(home_dir()?.join(".config/wake-clock/config.toml"))
}

pub fn data_dir() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("WAKE_CLOCK_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }
    Ok(home_dir()?.join(".local/share/wake-clock/records"))
}

fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".to_string())
}

pub fn ensure_default_config() -> Result<PathBuf, String> {
    let path = config_path()?;
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(error_string)?;
        }
        fs::write(&path, DEFAULT_CONFIG).map_err(error_string)?;
    }
    Ok(path)
}

pub fn load_config() -> Result<Config, String> {
    let path = ensure_default_config()?;
    let contents = fs::read_to_string(&path).map_err(error_string)?;
    let config: Config = toml::from_str(&contents)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &Config) -> Result<(), String> {
    if config.search.start_time > config.search.end_time {
        return Err("search.start_time must not be after search.end_time".into());
    }
    if config.schedule.rules.is_empty() {
        return Err("schedule.rules must contain at least one rule".into());
    }
    if config.schedule.reminder_minutes < 0 {
        return Err("schedule.reminder_minutes must be non-negative".into());
    }
    if config.automation.daily_record_time < config.search.end_time {
        return Err("automation.daily_record_time must not be before search.end_time".into());
    }
    if config.automation.reminder_check_seconds < 30 {
        return Err("automation.reminder_check_seconds must be at least 30".into());
    }
    if config
        .schedule
        .rules
        .windows(2)
        .any(|pair| pair[0].until >= pair[1].until)
    {
        return Err("schedule.rules must be ordered by increasing `until` values".into());
    }
    if config.schedule.rules.last().unwrap().until < config.search.end_time {
        return Err("the final schedule rule must cover search.end_time".into());
    }
    for rule in &config.schedule.rules {
        match rule.kind {
            RuleKind::AddDuration if rule.duration_minutes.is_none_or(|duration| duration < 0) => {
                return Err("add_duration rule requires non-negative duration_minutes".into());
            }
            RuleKind::Fixed if rule.end_time.is_none() => {
                return Err("fixed rule requires end_time".into());
            }
            RuleKind::CarryDelay if rule.anchor_start.is_none() || rule.anchor_end.is_none() => {
                return Err("carry_delay rule requires anchor_start and anchor_end".into());
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn read_pmset_log() -> Result<String, String> {
    let output = Command::new("/usr/bin/pmset")
        .args(["-g", "log"])
        .output()
        .map_err(error_string)?;
    if !output.status.success() {
        return Err(format!(
            "pmset failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn select_wake_event(log: &str, date: NaiveDate, search: &SearchConfig) -> Option<NaiveTime> {
    let date_prefix = date.format("%Y-%m-%d").to_string();
    log.lines()
        .filter(|line| line.starts_with(&date_prefix) && line.contains("Display is turned on"))
        .filter_map(|line| line.split_whitespace().nth(1))
        .filter_map(|value| parse_time(value).ok())
        .filter(|time| *time >= search.start_time && *time <= search.end_time)
        .min_by_key(|time| {
            time.signed_duration_since(search.target_time)
                .num_seconds()
                .abs()
        })
}

pub fn estimate_end_time(wake: NaiveTime, schedule: &ScheduleConfig) -> Result<NaiveTime, String> {
    let rule = schedule
        .rules
        .iter()
        .find(|rule| wake <= rule.until)
        .ok_or_else(|| format!("no schedule rule matches wake time {wake}"))?;

    let calculated = match rule.kind {
        RuleKind::AddDuration => {
            wake.overflowing_add_signed(TimeDelta::minutes(rule.duration_minutes.unwrap()))
                .0
        }
        RuleKind::Fixed => rule.end_time.unwrap(),
        RuleKind::CarryDelay => {
            let delay = wake.signed_duration_since(rule.anchor_start.unwrap());
            rule.anchor_end.unwrap().overflowing_add_signed(delay).0
        }
    };

    Ok(calculated.min(schedule.max_end_time))
}

pub fn record_path(date: NaiveDate) -> Result<PathBuf, String> {
    Ok(data_dir()?.join(format!("{}.json", date.format("%Y-%m-%d"))))
}

pub fn load_record(date: NaiveDate) -> Result<Option<DailyRecord>, String> {
    let path = record_path(date)?;
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map(Some)
            .map_err(|error| format!("failed to parse {}: {error}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error_string(error)),
    }
}

pub fn save_record(record: &DailyRecord) -> Result<PathBuf, String> {
    let path = record_path(record.date)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error_string)?;
    }
    let json = serde_json::to_string_pretty(record).map_err(error_string)?;
    fs::write(&path, format!("{json}\n")).map_err(error_string)?;
    Ok(path)
}

pub fn build_record(
    date: NaiveDate,
    wake_time: NaiveTime,
    end_time: NaiveTime,
    reminder_minutes: i64,
    previous: Option<DailyRecord>,
) -> DailyRecord {
    let unchanged = previous.as_ref().is_some_and(|record| {
        record.wake_time == wake_time && record.estimated_end_time == end_time
    });
    DailyRecord {
        date,
        wake_time,
        estimated_end_time: end_time,
        source: "pmset: Display is turned on".into(),
        reminder_minutes,
        reminder_sent: unchanged && previous.as_ref().is_some_and(|record| record.reminder_sent),
        reminder_sent_at: if unchanged {
            previous.and_then(|record| record.reminder_sent_at)
        } else {
            None
        },
    }
}

pub fn should_remind(record: &DailyRecord, now: NaiveTime) -> bool {
    if record.reminder_sent {
        return false;
    }
    let reminder_time = record
        .estimated_end_time
        .overflowing_sub_signed(TimeDelta::minutes(record.reminder_minutes))
        .0;
    now >= reminder_time && now < record.estimated_end_time
}

pub fn send_notification(record: &DailyRecord) -> Result<(), String> {
    let message = format!(
        "预计 {} 下班，还有 {} 分钟。今日亮屏打卡约 {}。",
        record.estimated_end_time.format("%H:%M"),
        record.reminder_minutes,
        record.wake_time.format("%H:%M")
    );
    let script = format!(
        "display notification \"{}\" with title \"Wake Clock\"",
        escape_applescript(&message)
    );
    let status = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .status()
        .map_err(error_string)?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("osascript exited with {status}"))
    }
}

fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn record_today(force: bool) -> Result<DailyRecord, String> {
    let config = load_config()?;
    let now = Local::now();
    let date = now.date_naive();
    let previous = load_record(date)?;
    if !force && let Some(record) = previous {
        return Ok(record);
    }
    let log = read_pmset_log()?;
    let wake_time = select_wake_event(&log, date, &config.search).ok_or_else(|| {
        format!(
            "no display-on event found between {} and {} on {date}",
            config.search.start_time, config.search.end_time
        )
    })?;
    let end_time = estimate_end_time(wake_time, &config.schedule)?;
    let record = build_record(
        date,
        wake_time,
        end_time,
        config.schedule.reminder_minutes,
        previous,
    );
    save_record(&record)?;
    Ok(record)
}

pub fn scheduled_record_is_due(now: NaiveTime, config: &Config) -> bool {
    now >= config.automation.daily_record_time
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReminderOutcome {
    Sent,
    AlreadySent,
    NoRecord,
    NotDue { reminder_time: NaiveTime },
    EndTimePassed,
}

pub fn remind_today() -> Result<ReminderOutcome, String> {
    let now = Local::now();
    let date = now.date_naive();
    let Some(mut record) = load_record(date)? else {
        return Ok(ReminderOutcome::NoRecord);
    };
    if record.reminder_sent {
        return Ok(ReminderOutcome::AlreadySent);
    }
    if now.time() >= record.estimated_end_time {
        return Ok(ReminderOutcome::EndTimePassed);
    }
    let reminder_time = record
        .estimated_end_time
        .overflowing_sub_signed(TimeDelta::minutes(record.reminder_minutes))
        .0;
    if now.time() < reminder_time {
        return Ok(ReminderOutcome::NotDue { reminder_time });
    }

    send_notification(&record)?;
    record.reminder_sent = true;
    record.reminder_sent_at = Some(now.to_rfc3339());
    save_record(&record)?;
    Ok(ReminderOutcome::Sent)
}

pub fn run_daemon() -> ! {
    loop {
        let sleep_seconds = match load_config() {
            Ok(config) => {
                let now = Local::now();
                let date = now.date_naive();
                match load_record(date) {
                    Ok(None) if scheduled_record_is_due(now.time(), &config) => {
                        if let Err(error) = record_today(false) {
                            eprintln!("Automatic record failed: {error}");
                        }
                    }
                    Err(error) => eprintln!("Could not read today's record: {error}"),
                    _ => {}
                }
                if let Err(error) = remind_today() {
                    eprintln!("Automatic reminder failed: {error}");
                }
                config.automation.reminder_check_seconds
            }
            Err(error) => {
                eprintln!("Could not load configuration: {error}");
                300
            }
        };
        thread::sleep(Duration::from_secs(sleep_seconds));
    }
}

pub fn list_records(limit: usize) -> Result<Vec<DailyRecord>, String> {
    let directory = data_dir()?;
    let mut paths = match fs::read_dir(&directory) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect::<Vec<_>>(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error_string(error)),
    };
    paths.sort_by(|left, right| right.cmp(left));
    paths
        .into_iter()
        .take(limit)
        .map(|path| {
            let contents = fs::read_to_string(&path).map_err(error_string)?;
            serde_json::from_str(&contents).map_err(error_string)
        })
        .collect()
}

const RECORD_LABEL: &str = "com.weierz.wake-clock.record";
const REMINDER_LABEL: &str = "com.weierz.wake-clock.reminder";

#[derive(Debug)]
pub struct AutomationInstallation {
    pub executable: PathBuf,
    pub record_agent: PathBuf,
    pub reminder_agent: PathBuf,
}

pub fn install_automation(config: &Config) -> Result<AutomationInstallation, String> {
    let home = home_dir()?;
    let source_executable = env::current_exe().map_err(error_string)?;
    let bin_dir = home.join(".local/bin");
    fs::create_dir_all(&bin_dir).map_err(error_string)?;
    let executable = bin_dir.join("wake-clock");
    if !paths_refer_to_same_file(&source_executable, &executable) {
        fs::copy(&source_executable, &executable).map_err(|error| {
            format!(
                "failed to install {}: {error}",
                executable.to_string_lossy()
            )
        })?;
    }

    let data_root = home.join(".local/share/wake-clock");
    fs::create_dir_all(&data_root).map_err(error_string)?;
    let agents_dir = home.join("Library/LaunchAgents");
    fs::create_dir_all(&agents_dir).map_err(error_string)?;
    let record_agent = agents_dir.join(format!("{RECORD_LABEL}.plist"));
    let reminder_agent = agents_dir.join(format!("{REMINDER_LABEL}.plist"));

    fs::write(
        &record_agent,
        record_agent_plist(
            &executable,
            &home,
            &data_root,
            config.automation.daily_record_time,
        ),
    )
    .map_err(error_string)?;
    fs::write(
        &reminder_agent,
        reminder_agent_plist(
            &executable,
            &home,
            &data_root,
            config.automation.reminder_check_seconds,
        ),
    )
    .map_err(error_string)?;

    let domain = launchd_domain()?;
    reload_agent(&domain, RECORD_LABEL, &record_agent)?;
    if let Err(error) = reload_agent(&domain, REMINDER_LABEL, &reminder_agent) {
        bootout_agent(&domain, RECORD_LABEL);
        return Err(error);
    }

    Ok(AutomationInstallation {
        executable,
        record_agent,
        reminder_agent,
    })
}

pub fn uninstall_automation() -> Result<(), String> {
    let home = home_dir()?;
    let domain = launchd_domain()?;
    for label in [RECORD_LABEL, REMINDER_LABEL] {
        bootout_agent(&domain, label);
        remove_file_if_exists(&home.join(format!("Library/LaunchAgents/{label}.plist")))?;
    }
    remove_file_if_exists(&home.join(".local/bin/wake-clock"))?;
    Ok(())
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("failed to remove {}: {error}", path.display())),
    }
}

fn launchd_domain() -> Result<String, String> {
    let output = Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(error_string)?;
    if !output.status.success() {
        return Err("failed to determine the current user id".into());
    }
    Ok(format!(
        "gui/{}",
        String::from_utf8_lossy(&output.stdout).trim()
    ))
}

fn reload_agent(domain: &str, label: &str, plist: &Path) -> Result<(), String> {
    bootout_agent(domain, label);
    let status = Command::new("/bin/launchctl")
        .args(["bootstrap", domain, plist.to_string_lossy().as_ref()])
        .status()
        .map_err(error_string)?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "failed to load {} with launchctl ({status})",
            plist.display()
        ))
    }
}

fn bootout_agent(domain: &str, label: &str) {
    let service = format!("{domain}/{label}");
    let _ = Command::new("/bin/launchctl")
        .args(["bootout", &service])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn record_agent_plist(
    executable: &Path,
    home: &Path,
    data_root: &Path,
    record_time: NaiveTime,
) -> String {
    let calendar = format!(
        "  <key>StartCalendarInterval</key>\n  <dict>\n    <key>Hour</key>\n    <integer>{}</integer>\n    <key>Minute</key>\n    <integer>{}</integer>\n  </dict>\n  <key>RunAtLoad</key>\n  <true/>",
        record_time.format("%H"),
        record_time.format("%M")
    );
    launch_agent_plist(
        RECORD_LABEL,
        executable,
        &["record", "--scheduled", "--quiet"],
        home,
        data_root,
        &calendar,
    )
}

fn reminder_agent_plist(executable: &Path, home: &Path, data_root: &Path, interval: u64) -> String {
    let schedule = format!(
        "  <key>StartInterval</key>\n  <integer>{interval}</integer>\n  <key>RunAtLoad</key>\n  <true/>"
    );
    launch_agent_plist(
        REMINDER_LABEL,
        executable,
        &["remind", "--quiet"],
        home,
        data_root,
        &schedule,
    )
}

fn launch_agent_plist(
    label: &str,
    executable: &Path,
    arguments: &[&str],
    home: &Path,
    data_root: &Path,
    schedule: &str,
) -> String {
    let arguments = arguments
        .iter()
        .map(|argument| format!("    <string>{}</string>", xml_escape(argument)))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{executable}</string>
{arguments}
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>HOME</key>
    <string>{home}</string>
  </dict>
{schedule}
  <key>ProcessType</key>
  <string>Background</string>
  <key>StandardOutPath</key>
  <string>{data_root}/wake-clock.log</string>
  <key>StandardErrorPath</key>
  <string>{data_root}/wake-clock.error.log</string>
</dict>
</plist>
"#,
        label = xml_escape(label),
        executable = xml_escape(executable.to_string_lossy().as_ref()),
        home = xml_escape(home.to_string_lossy().as_ref()),
        data_root = xml_escape(data_root.to_string_lossy().as_ref()),
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn error_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use std::io::Write;

    fn config() -> Config {
        toml::from_str(DEFAULT_CONFIG).unwrap()
    }

    #[test]
    fn old_configs_receive_safe_automation_defaults() {
        let old_config = DEFAULT_CONFIG.split("[automation]").next().unwrap();
        let config: Config = toml::from_str(old_config).unwrap();
        assert_eq!(
            config.automation.daily_record_time,
            parse_time("14:05").unwrap()
        );
        assert_eq!(config.automation.reminder_check_seconds, 60);
    }

    #[test]
    fn scheduled_record_only_runs_after_the_configured_time() {
        let config = config();
        assert!(!scheduled_record_is_due(
            parse_time("14:04:59").unwrap(),
            &config
        ));
        assert!(scheduled_record_is_due(
            parse_time("14:05:00").unwrap(),
            &config
        ));
    }

    #[test]
    fn selects_event_closest_to_nine_and_ignores_midnight_wakes() {
        let log = r#"
2026-08-20 00:23:30 +0800 Notification Display is turned on
2026-08-20 08:30:00 +0800 Notification Display is turned on
2026-08-20 09:11:27 +0800 Notification Display is turned on
2026-08-20 10:45:00 +0800 Notification Display is turned on
"#;
        let date = NaiveDate::from_ymd_opt(2026, 8, 20).unwrap();
        assert_eq!(
            select_wake_event(log, date, &config().search),
            Some(parse_time("09:11:27").unwrap())
        );
    }

    #[test]
    fn applies_default_schedule_rules() {
        let schedule = config().schedule;
        let cases = [
            ("09:11:00", "18:11:00"),
            ("10:30:00", "18:00:00"),
            ("12:45:00", "18:00:00"),
            ("13:00:00", "18:00:00"),
            ("13:20:00", "18:20:00"),
            ("13:50:00", "18:50:00"),
            ("14:00:00", "19:00:00"),
        ];
        for (wake, expected) in cases {
            assert_eq!(
                estimate_end_time(parse_time(wake).unwrap(), &schedule).unwrap(),
                parse_time(expected).unwrap(),
                "wake time {wake}"
            );
        }
    }

    #[test]
    fn reminder_is_only_due_inside_the_reminder_window() {
        let record = DailyRecord {
            date: NaiveDate::from_ymd_opt(2026, 8, 20).unwrap(),
            wake_time: parse_time("09:00").unwrap(),
            estimated_end_time: parse_time("18:00").unwrap(),
            source: "test".into(),
            reminder_minutes: 30,
            reminder_sent: false,
            reminder_sent_at: None,
        };
        assert!(!should_remind(&record, parse_time("17:29").unwrap()));
        assert!(should_remind(&record, parse_time("17:30").unwrap()));
        assert!(!should_remind(&record, parse_time("18:00").unwrap()));
    }

    #[test]
    fn creates_separate_daily_record_and_reminder_agents() {
        let config = config();
        let executable = Path::new("/Users/a&b/.local/bin/wake-clock");
        let home = Path::new("/Users/a&b");
        let data = Path::new("/Users/a&b/.local/share/wake-clock");
        let record =
            record_agent_plist(executable, home, data, config.automation.daily_record_time);
        assert!(record.contains("com.weierz.wake-clock.record"));
        assert!(record.contains("<integer>14</integer>"));
        assert!(record.contains("<integer>05</integer>"));
        assert!(record.contains("/Users/a&amp;b/.local/bin/wake-clock"));
        assert!(record.contains("<string>--scheduled</string>"));
        assert!(record.contains("<key>RunAtLoad</key>"));

        let reminder = reminder_agent_plist(executable, home, data, 60);
        assert!(reminder.contains("com.weierz.wake-clock.reminder"));
        assert!(reminder.contains("<integer>60</integer>"));
        assert!(reminder.contains("<string>--quiet</string>"));

        #[cfg(target_os = "macos")]
        {
            assert_valid_plist(&record);
            assert_valid_plist(&reminder);
        }
    }

    #[cfg(target_os = "macos")]
    fn assert_valid_plist(plist: &str) {
        let mut child = Command::new("/usr/bin/plutil")
            .args(["-lint", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(plist.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
    }
}
