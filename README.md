# Work Work

**A tiny macOS helper that estimates when your workday ends and keeps a lightweight work-hours journal.**

Work Work reads local display events from macOS, records a reasonable first display-on and last display-off for each day, and sends a notification when it is almost time to wrap up.

```console
$ ww
18:11:27
```

No account, server, or manual check-in is required. Everything stays on your Mac.

## What it does

1. Reads `Display is turned on` events from `pmset -g log`.
2. Selects the first event inside your configured start window.
3. Applies your schedule rules to estimate an end time.
4. Captures the last display-off event inside your configured end window.
5. Calculates net work time after subtracting the configured break overlap.
6. Schedules one macOS notification for the reminder time.
7. On the 15th, summarizes the previous month and marks unusual days.

Work Work records the day when your Mac session becomes active. Running `ww` also creates today's record immediately when one does not exist, while `14:05` remains a fallback. It then schedules a notification for 10 minutes before the estimated end time. The display hook waits for macOS events and does not poll.

> [!NOTE]
> Work Work provides a convenient estimate. It is not an attendance system or an authoritative time tracker.

## Default schedule

The default start window is `05:00–14:00`. Within that window, Work Work selects the first display-on event. Events such as an automatic midnight wake are therefore ignored.

| Selected wake time | Estimated end time |
| --- | --- |
| Before 10:00 | Wake time + 9 hours |
| 10:00–13:00 | 18:00 |
| 13:00–14:00 | 18:00 plus the time after 13:00 |
| Any calculated result | Never later than 19:00 |

Examples:

| Wake | End |
| --- | --- |
| 09:11 | 18:11 |
| 10:30 | 18:00 |
| 13:20 | 18:20 |
| 14:00 | 19:00 |

Every part of this schedule can be changed in the configuration file.

## Installation

Work Work requires an Apple Silicon Mac. The one-line installer uses a prebuilt binary, so Rust is not required.

### One-line installer

The installer downloads the latest Apple Silicon `ww` binary, creates the default configuration, and enables automatic recording and notifications:

```bash
curl -fsSL https://raw.githubusercontent.com/weierz/work-work/main/install.sh | zsh
```

The Apple Silicon archive is also available on the [Releases page](https://github.com/weierz/work-work/releases).

### Build from source

Building from source requires a Rust toolchain:

```bash
git clone https://github.com/weierz/work-work.git
cd work-work
./install.sh
```

The installer uses user-level LaunchAgents and does not require administrator privileges.

### Homebrew

```bash
brew install weierz/tap/work-work && brew services start weierz/tap/work-work
```

To stop and remove the Homebrew installation:

```bash
brew services stop weierz/tap/work-work
brew uninstall weierz/tap/work-work
```

## Everyday use

Once installed, Work Work records and reminds you automatically. You only need the CLI when you want to check something. If today's record is missing, plain `ww` creates it immediately from today's existing display events before printing the estimated end time.

```bash
ww              # Print only today's estimated end time
ww status       # Show today's full record
ww history      # Show the latest 14 records
ww history 30   # Show the latest 30 records
ww month        # Summarize the previous month
ww month 2026-07 # Summarize a specific month
```

Useful maintenance commands:

```bash
ww record           # Create today's record if it does not exist
ww record --force   # Recalculate today's record
ww remind           # Check the reminder immediately
ww init              # Create the default configuration
ww install           # Reload source-install automation after config changes
ww uninstall         # Remove source-install automation, keeping config and data
ww help
```

## Configuration

The default configuration is created at:

```text
~/.config/work-work/config.toml
```

See [`config.example.toml`](config.example.toml) for the complete example.

| Setting | Purpose | Default |
| --- | --- | --- |
| `search.start_time` | Start of the wake-event search window | `05:00:00` |
| `search.end_time` | End of the wake-event search window | `14:00:00` |
| `time_accounting.reasonable_end_start` | Start of the display-off window | `15:00:00` |
| `time_accounting.reasonable_end_end` | End of the display-off window | `23:00:00` |
| `time_accounting.break_start` | Start of the unpaid break | `12:00:00` |
| `time_accounting.break_end` | End of the unpaid break | `13:00:00` |
| `time_accounting.daily_target_minutes` | Expected work time per recorded day | `480` |
| `time_accounting.anomaly_threshold_minutes` | Difference from the target that marks a day unusual | `60` |
| `time_accounting.monthly_summary_day` | Day to summarize the previous month | `15` |
| `schedule.max_end_time` | Latest allowed estimated end time | `19:00:00` |
| `schedule.reminder_minutes` | Minutes before the end time to notify | `10` |
| `automation.daily_record_time` | Fallback time to create the daily record | `14:05:00` |

### Schedule rules

Rules are evaluated from top to bottom. The first rule whose `until` value includes the selected wake time is used.

Available rule types:

- `add_duration`: add a number of minutes to the wake time.
- `fixed`: use a fixed end time.
- `carry_delay`: add the delay after one anchor to another anchor.

For example, this rule turns `13:20` into `18:20`:

```toml
[[schedule.rules]]
until = "14:00:00"
kind = "carry_delay"
anchor_start = "13:00:00"
anchor_end = "18:00:00"
```

After changing the automation or reminder settings for a source installation, reload the LaunchAgents:

```bash
ww install
```

### Work-hours accounting

Work time is calculated as:

```text
last reasonable display-off − first reasonable display-on − overlapping break time
```

With the defaults, `09:00–18:00` becomes 8 hours after the `12:00–13:00` break. A day is marked as unusual when it differs from the 8-hour target by at least 60 minutes, or when no reasonable display-off event was found.

On the configured summary day, Work Work sends one notification for the previous month. The monthly balance includes every day that has a daily record. It deliberately does not interpret weekends, public holidays, or leave; unrecorded dates are simply absent, while a recorded date without a display-off event is marked as an anomaly.

The last display-off event is finalized by the following day's record task, so no frequent background check is needed. The display hook blocks until macOS reports that the session became active; it does not wake on an interval.

## Data and privacy

Work Work only reads local macOS power-management logs. It does not send your activity or records anywhere.

Daily records:

```text
~/.local/share/work-work/records/YYYY-MM-DD.json
```

Background logs:

```text
~/.local/share/work-work/work-work.log
~/.local/share/work-work/work-work.error.log
```

Example record:

```json
{
  "date": "2026-08-20",
  "wake_time": "09:11:27",
  "estimated_end_time": "18:11:27",
  "source": "pmset: Display is turned on",
  "reminder_minutes": 10,
  "reminder_sent": false,
  "reminder_sent_at": null,
  "display_off_time": "18:21:30",
  "work_minutes": 490
}
```

Monthly summaries:

```text
~/.local/share/work-work/monthly/YYYY-MM.json
```

For scripts and temporary environments, the default paths can be overridden:

```bash
WW_CONFIG=/path/to/config.toml
WW_DATA_DIR=/path/to/records
```

## Troubleshooting

If no notification appears, make sure macOS allows `osascript` notifications and inspect:

```text
~/.local/share/work-work/work-work.error.log
```

If the recorded times look wrong, inspect today's display events and adjust the start or end window:

```bash
pmset -g log | grep -E "$(date +%Y-%m-%d).*Display is turned (on|off)"
```

## Development

```bash
cargo fmt --all -- --check
cargo check --locked
cargo test --locked
```

## Requirements and limitations

- Apple Silicon macOS only; Work Work relies on the built-in `pmset` command.
- Display events are an approximation of when work began and ended.
- Short automatic events can occur, so the configured start and end windows matter.
- Monthly summaries do not infer workdays, holidays, or leave.
- Notifications require permission for `osascript` to post notifications.

## License

[MIT](LICENSE)
