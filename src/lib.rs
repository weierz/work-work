use chrono::{Local, NaiveDate, NaiveTime, TimeDelta};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

pub const DEFAULT_CONFIG: &str = include_str!("../config.example.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub search: SearchConfig,
    pub schedule: ScheduleConfig,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReminderOutcome {
    Sent,
    AlreadySent,
    NotDue { reminder_time: NaiveTime },
    EndTimePassed,
}

pub fn remind_today() -> Result<ReminderOutcome, String> {
    let now = Local::now();
    let date = now.date_naive();
    let mut record = load_record(date)?
        .ok_or_else(|| format!("no record for {date}; run `wake-clock record` first"))?;
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

fn error_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        toml::from_str(DEFAULT_CONFIG).unwrap()
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
}
