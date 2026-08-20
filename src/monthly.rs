use super::{
    Config, DailyRecord, TimeAccountingConfig, data_dir, error_string, escape_applescript,
    list_records,
};
use chrono::{Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DailyAnomaly {
    pub date: NaiveDate,
    pub kind: String,
    pub work_minutes: Option<i64>,
    pub difference_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MonthlyReport {
    pub month: String,
    pub recorded_days: usize,
    pub completed_days: usize,
    pub total_minutes: i64,
    pub target_minutes: i64,
    pub balance_minutes: i64,
    pub anomalies: Vec<DailyAnomaly>,
    pub notified_at: Option<String>,
}

pub fn summarize_month(
    month_start: NaiveDate,
    records: &[DailyRecord],
    accounting: &TimeAccountingConfig,
) -> MonthlyReport {
    let mut report = MonthlyReport {
        month: month_start.format("%Y-%m").to_string(),
        recorded_days: 0,
        completed_days: 0,
        total_minutes: 0,
        target_minutes: 0,
        balance_minutes: 0,
        anomalies: Vec::new(),
        notified_at: None,
    };

    for record in records
        .iter()
        .filter(|record| is_in_month(record.date, month_start))
    {
        report.recorded_days += 1;
        report.target_minutes += accounting.daily_target_minutes;
        if let Some(minutes) = record.work_minutes {
            report.completed_days += 1;
            report.total_minutes += minutes;
        }
        if let Some(anomaly) = anomaly_for(record, accounting) {
            report.anomalies.push(anomaly);
        }
    }

    report.balance_minutes = report.total_minutes - report.target_minutes;
    report
}

fn is_in_month(date: NaiveDate, month_start: NaiveDate) -> bool {
    date.year() == month_start.year() && date.month() == month_start.month()
}

fn anomaly_for(record: &DailyRecord, accounting: &TimeAccountingConfig) -> Option<DailyAnomaly> {
    let Some(minutes) = record.work_minutes else {
        return Some(DailyAnomaly {
            date: record.date,
            kind: "missing_display_off".into(),
            work_minutes: None,
            difference_minutes: None,
        });
    };
    let difference = minutes - accounting.daily_target_minutes;
    if difference.abs() < accounting.anomaly_threshold_minutes {
        return None;
    }
    Some(DailyAnomaly {
        date: record.date,
        kind: if difference < 0 {
            "short_day".into()
        } else {
            "long_day".into()
        },
        work_minutes: Some(minutes),
        difference_minutes: Some(difference),
    })
}

fn monthly_dir() -> Result<PathBuf, String> {
    let records = data_dir()?;
    let root = records
        .parent()
        .ok_or_else(|| "records directory has no parent".to_string())?;
    Ok(root.join("monthly"))
}

fn monthly_report_path(month_start: NaiveDate) -> Result<PathBuf, String> {
    Ok(monthly_dir()?.join(format!("{}.json", month_start.format("%Y-%m"))))
}

fn load_saved_monthly_report(month_start: NaiveDate) -> Result<Option<MonthlyReport>, String> {
    let path = monthly_report_path(month_start)?;
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map(Some)
            .map_err(|error| format!("failed to parse {}: {error}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error_string(error)),
    }
}

fn save_monthly_report(report: &MonthlyReport) -> Result<PathBuf, String> {
    let month_start = parse_month(&report.month)?;
    let path = monthly_report_path(month_start)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error_string)?;
    }
    let json = serde_json::to_string_pretty(report).map_err(error_string)?;
    fs::write(&path, format!("{json}\n")).map_err(error_string)?;
    Ok(path)
}

pub fn parse_month(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d")
        .map_err(|_| format!("invalid month `{value}`; expected YYYY-MM"))
}

pub fn previous_month(date: NaiveDate) -> NaiveDate {
    date.with_day(1)
        .unwrap()
        .pred_opt()
        .unwrap()
        .with_day(1)
        .unwrap()
}

pub fn generate_monthly_report(
    month_start: NaiveDate,
    accounting: &TimeAccountingConfig,
) -> Result<MonthlyReport, String> {
    let records = list_records(10_000)?;
    let mut report = summarize_month(month_start, &records, accounting);
    report.notified_at =
        load_saved_monthly_report(month_start)?.and_then(|existing| existing.notified_at);
    save_monthly_report(&report)?;
    Ok(report)
}

fn send_monthly_notification(report: &MonthlyReport) -> Result<(), String> {
    let message = format!(
        "{}: {} recorded days, {}, balance {}, {} anomalies.",
        report.month,
        report.recorded_days,
        format_minutes(report.total_minutes),
        format_signed_minutes(report.balance_minutes),
        report.anomalies.len()
    );
    let script = format!(
        "display notification \"{}\" with title \"Work Work Monthly Summary\"",
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

pub fn format_minutes(minutes: i64) -> String {
    format!("{}h {:02}m", minutes / 60, minutes.abs() % 60)
}

pub fn format_signed_minutes(minutes: i64) -> String {
    let sign = if minutes >= 0 { "+" } else { "-" };
    format!("{sign}{}h {:02}m", minutes.abs() / 60, minutes.abs() % 60)
}

pub fn maybe_send_monthly_summary(
    now: chrono::DateTime<Local>,
    config: &Config,
) -> Result<(), String> {
    if now.day() != config.time_accounting.monthly_summary_day {
        return Ok(());
    }
    let month = previous_month(now.date_naive());
    let mut report = generate_monthly_report(month, &config.time_accounting)?;
    if report.recorded_days == 0 || report.notified_at.is_some() {
        return Ok(());
    }
    send_monthly_notification(&report)?;
    report.notified_at = Some(now.to_rfc3339());
    save_monthly_report(&report)?;
    Ok(())
}
