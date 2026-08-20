use chrono::Local;
use screen_wake_clock::{
    ReminderOutcome, ensure_default_config, install_automation, list_records, load_config,
    load_record, record_today, remind_today, run_daemon, scheduled_record_is_due,
    uninstall_automation,
};
use std::env;
use std::process::ExitCode;

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
    let command = args.get(1).map(String::as_str).unwrap_or("status");
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
                None => println!(
                    "No record for {today}. Run `wake-clock record` once after the search window."
                ),
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
                    "{}  wake {}  end {}{}",
                    record.date,
                    record.wake_time.format("%H:%M:%S"),
                    record.estimated_end_time.format("%H:%M:%S"),
                    if record.reminder_sent {
                        "  reminded"
                    } else {
                        ""
                    }
                );
            }
        }
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
        other => return Err(format!("unknown command `{other}`; run wake-clock help")),
    }
    Ok(())
}

fn print_record(record: &screen_wake_clock::DailyRecord) {
    println!(
        "{}  wake {}  estimated end {}  reminder {}",
        record.date,
        record.wake_time.format("%H:%M:%S"),
        record.estimated_end_time.format("%H:%M:%S"),
        if record.reminder_sent {
            "sent"
        } else {
            "pending"
        }
    );
}

fn print_help() {
    println!(
        "wake-clock — estimate clock-out time from macOS display wake events\n\n\
         Usage:\n  \
         wake-clock init           Create the default config\n  \
         wake-clock install        Enable automatic daily recording and reminders\n  \
         wake-clock uninstall      Disable automation and keep config/history\n  \
         wake-clock record         Create today's record once\n  \
         wake-clock record --force Recalculate and replace today's record\n  \
         wake-clock remind         Send the reminder when it is due\n  \
         wake-clock status         Show today's record\n  \
         wake-clock history [N]    Show the latest N records (default 14)\n"
    );
}
