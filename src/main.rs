use chrono::Local;
use std::env;
use std::process::ExitCode;
use work_work::{
    MonthlyReport, ReminderOutcome, ensure_default_config, format_minutes, format_signed_minutes,
    generate_monthly_report, install_automation, list_records, load_config, load_record,
    parse_month, previous_month, record_today, remind_today, run_automation_tick, run_daemon,
    scheduled_record_is_due, uninstall_automation,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = env::args().collect::<Vec<_>>();
    let Some(command) = args.get(1).map(String::as_str) else {
        let today = Local::now().date_naive();
        let record = load_record(today)?
            .ok_or_else(|| format!("no record for {today}; automatic recording has not run yet"))?;
        println!("{}", record.estimated_end_time.format("%H:%M:%S"));
        return Ok(());
    };
    match command {
        "init" => {
            let path = ensure_default_config()?;
            println!("Config: {}", path.display());
        }
        "record" => {
            let force = args.iter().any(|argument| argument == "--force");
            let scheduled = args.iter().any(|argument| argument == "--scheduled");
            let quiet = args.iter().any(|argument| argument == "--quiet");
            if scheduled {
                let config = load_config()?;
                if !scheduled_record_is_due(Local::now().time(), &config) {
                    return Ok(());
                }
            }
            let record = record_today(force)?;
            if !quiet {
                print_record(&record);
            }
        }
        "remind" => {
            let quiet = args.iter().any(|argument| argument == "--quiet");
            match remind_today()? {
                ReminderOutcome::Sent => println!("Reminder sent"),
                ReminderOutcome::AlreadySent if !quiet => println!("Reminder already sent"),
                ReminderOutcome::NoRecord if !quiet => println!("No record for today yet"),
                ReminderOutcome::NotDue { reminder_time } if !quiet => {
                    println!("Reminder is due at {}", reminder_time.format("%H:%M:%S"))
                }
                ReminderOutcome::EndTimePassed if !quiet => {
                    println!("Estimated end time has already passed")
                }
                _ => {}
            }
        }
        "status" => {
            let today = Local::now().date_naive();
            match load_record(today)? {
                Some(record) => print_record(&record),
                None => {
                    println!("No record for {today}. Run `ww record` once after the search window.")
                }
            }
        }
        "history" => {
            let limit = env::args()
                .nth(2)
                .map(|value| value.parse::<usize>())
                .transpose()
                .map_err(|_| "history limit must be a positive integer".to_string())?
                .unwrap_or(14);
            for record in list_records(limit)? {
                println!(
                    "{}  on {}  off {}  worked {}{}",
                    record.date,
                    record.wake_time.format("%H:%M:%S"),
                    record
                        .display_off_time
                        .map(|time| time.format("%H:%M:%S").to_string())
                        .unwrap_or_else(|| "--".into()),
                    record
                        .work_minutes
                        .map(format_minutes)
                        .unwrap_or_else(|| "--".into()),
                    if record.reminder_sent {
                        "  reminded"
                    } else {
                        ""
                    }
                );
            }
        }
        "month" => {
            let config = load_config()?;
            let month = args
                .get(2)
                .map(|value| parse_month(value))
                .transpose()?
                .unwrap_or_else(|| previous_month(Local::now().date_naive()));
            print_monthly_report(&generate_monthly_report(month, &config.time_accounting)?);
        }
        "tick" => run_automation_tick()?,
        "install" => {
            let installation = install_automation(&load_config()?)?;
            println!("Automatic daily recording and reminders are enabled.");
            println!("Executable: {}", installation.executable.display());
            println!("Record agent: {}", installation.record_agent.display());
            println!("Reminder agent: {}", installation.reminder_agent.display());
        }
        "uninstall" => {
            uninstall_automation()?;
            println!("Automatic recording and reminders are disabled.");
            println!("Configuration and history were kept.");
        }
        "daemon" => run_daemon(),
        "help" | "--help" | "-h" => print_help(),
        other => return Err(format!("unknown command `{other}`; run ww help")),
    }
    Ok(())
}

fn print_record(record: &work_work::DailyRecord) {
    println!(
        "{}  on {}  off {}  worked {}  estimated end {}  reminder {}",
        record.date,
        record.wake_time.format("%H:%M:%S"),
        record
            .display_off_time
            .map(|time| time.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "--".into()),
        record
            .work_minutes
            .map(format_minutes)
            .unwrap_or_else(|| "--".into()),
        record.estimated_end_time.format("%H:%M:%S"),
        if record.reminder_sent {
            "sent"
        } else {
            "pending"
        }
    );
}

fn print_monthly_report(report: &MonthlyReport) {
    println!("{}", report.month);
    println!("Recorded days  {}", report.recorded_days);
    println!("Completed days {}", report.completed_days);
    println!("Worked         {}", format_minutes(report.total_minutes));
    println!("Target         {}", format_minutes(report.target_minutes));
    println!(
        "Balance        {}",
        format_signed_minutes(report.balance_minutes)
    );
    println!("Anomalies      {}", report.anomalies.len());
    for anomaly in &report.anomalies {
        let detail = anomaly
            .work_minutes
            .map(format_minutes)
            .unwrap_or_else(|| "missing display-off event".into());
        println!("  {}  {}  {}", anomaly.date, anomaly.kind, detail);
    }
}

fn print_help() {
    println!(
        "ww — estimate clock-out time from macOS display wake events\n\n\
         Usage:\n  \
         ww                Print only today's estimated end time\n  \
         ww init           Create the default config\n  \
         ww install        Enable automatic daily recording and reminders\n  \
         ww uninstall      Disable automation and keep config/history\n  \
         ww record         Create today's record once\n  \
         ww record --force Recalculate and replace today's record\n  \
         ww remind         Send the reminder when it is due\n  \
         ww status         Show today's record\n  \
         ww history [N]    Show the latest N records (default 14)\n  \
         ww month [YYYY-MM] Show a monthly work-hours summary\n"
    );
}
