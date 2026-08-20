# Work Work

**A tiny macOS helper that estimates when your workday ends.**

Work Work reads local display-wake events from macOS, keeps one lightweight record per day, and sends a notification when it is almost time to wrap up.

```console
$ ww
18:11:27
```

No account, server, or manual check-in is required. Everything stays on your Mac.

## What it does

1. Reads `Display is turned on` events from `pmset -g log`.
2. Selects the event closest to your configured start time.
3. Applies your schedule rules to estimate an end time.
4. Saves one local JSON record for the day.
5. Sends one macOS notification before the estimated end time.

By default, Work Work records the day at `14:05` and sends a notification 10 minutes before the estimated end time.

> [!NOTE]
> Work Work provides a convenient estimate. It is not an attendance system or an authoritative time tracker.

## Default schedule

The default search window is `05:00–14:00`. Within that window, Work Work selects the display-wake event closest to `09:00`.

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

Work Work requires macOS and a Rust toolchain.

### One-line installer

The installer downloads the project, builds `ww`, creates the default configuration, and enables automatic recording and notifications:

```bash
curl -fsSL https://raw.githubusercontent.com/weierz/work-work/main/install.sh | zsh
```

### From a clone

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

Once installed, Work Work records and reminds you automatically. You only need the CLI when you want to check something.

```bash
ww              # Print only today's estimated end time
ww status       # Show today's full record
ww history      # Show the latest 14 records
ww history 30   # Show the latest 30 records
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
| `search.target_time` | Preferred start time used to rank wake events | `09:00:00` |
| `search.start_time` | Start of the wake-event search window | `05:00:00` |
| `search.end_time` | End of the wake-event search window | `14:00:00` |
| `schedule.max_end_time` | Latest allowed estimated end time | `19:00:00` |
| `schedule.reminder_minutes` | Minutes before the end time to notify | `10` |
| `automation.daily_record_time` | Time to create the daily record | `14:05:00` |
| `automation.reminder_check_seconds` | Reminder check interval | `60` |

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

After changing an automation time or interval for a source installation, reload the LaunchAgents:

```bash
ww install
```

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
  "reminder_sent_at": null
}
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

If the estimated time looks wrong, inspect the wake events for today and adjust the search window or target time:

```bash
pmset -g log | grep "$(date +%Y-%m-%d).*Display is turned on"
```

## Development

```bash
cargo fmt --all -- --check
cargo check --locked
cargo test --locked
```

## Requirements and limitations

- macOS only; Work Work relies on the built-in `pmset` command.
- Display-wake events are an approximation of when work began.
- Short automatic wake events can occur, so the search window and target time matter.
- Notifications require permission for `osascript` to post notifications.

## License

[MIT](LICENSE)
