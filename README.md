# Screen Wake Clock

一个面向 macOS 的 Rust 命令行工具。它从 `pmset -g log` 中读取屏幕亮起事件，估算当天的大致打卡时间和下班时间，并把结果保存为每日 JSON 记录。

当前版本只做 CLI，不会安装或修改 launchd、cron 等系统定时任务。

## 默认规则

候选事件限定在 `05:00–14:00`，从中选择距离 `09:00` 最近的一次 `Display is turned on`：

| 亮屏时间 | 预计下班时间 |
| --- | --- |
| 10:00 之前 | 亮屏时间 + 9 小时 |
| 10:00–13:00 | 18:00 |
| 13:00–14:00 | 18:00 + 超过 13:00 的时长 |
| 任意规则结果 | 最晚不超过 19:00 |

例如：`09:11 → 18:11`、`10:30 → 18:00`、`13:20 → 18:20`、`14:00 → 19:00`。

## 安装

需要 macOS 和 Rust 工具链：

```bash
git clone https://github.com/weierz/screen-wake-clock.git
cd screen-wake-clock
cargo install --path .
```

## 使用

建议在候选时间窗结束后每天执行一次记录命令：

```bash
wake-clock init
wake-clock record
```

同一天再次执行 `record` 只返回已有记录，不会覆盖。确认需要重新读取日志时才使用：

```bash
wake-clock record --force
```

其他命令：

```bash
wake-clock status       # 查看今天的记录
wake-clock history      # 查看最近 14 条记录
wake-clock history 30   # 查看最近 30 条记录
wake-clock remind       # 到提醒窗口时发送一次 macOS 通知
wake-clock help
```

`remind` 与 `record` 完全分离：记录每天运行一次即可；提醒命令可以在后续版本中交给 launchd 定时检查。当前版本不会自动创建任何系统任务。

## 配置

首次运行 `init` 或 `record` 会创建：

```text
~/.config/wake-clock/config.toml
```

完整示例见 [`config.example.toml`](config.example.toml)。主要配置项：

- `search.target_time`：选择亮屏事件时的目标时间。
- `search.start_time` / `search.end_time`：候选事件时间窗。
- `schedule.max_end_time`：最晚下班时间。
- `schedule.reminder_minutes`：提前多少分钟提醒。
- `schedule.rules`：按顺序匹配的下班时间规则。

支持三种规则：

- `add_duration`：在亮屏时间上增加指定分钟数。
- `fixed`：使用固定下班时间。
- `carry_delay`：把超过锚点的时长加到另一个锚点上。

例如，`13:20 → 18:20` 对应：

```toml
[[schedule.rules]]
until = "14:00:00"
kind = "carry_delay"
anchor_start = "13:00:00"
anchor_end = "18:00:00"
```

规则按配置中的顺序执行，第一条满足 `wake_time <= until` 的规则生效。

## 数据

每日记录保存在：

```text
~/.local/share/wake-clock/records/YYYY-MM-DD.json
```

示例：

```json
{
  "date": "2026-08-20",
  "wake_time": "09:11:27",
  "estimated_end_time": "18:11:27",
  "source": "pmset: Display is turned on",
  "reminder_minutes": 30,
  "reminder_sent": false,
  "reminder_sent_at": null
}
```

可用以下环境变量临时改写路径，便于测试或脚本集成：

```bash
WAKE_CLOCK_CONFIG=/path/to/config.toml
WAKE_CLOCK_DATA_DIR=/path/to/records
```

## 开发

```bash
cargo fmt --all -- --check
cargo check --locked
cargo test --locked
```

## 限制

- 只支持 macOS，因为数据来自系统自带的 `pmset`。
- 亮屏时间只是近似打卡时间，不等同于考勤系统记录。
- macOS 可能产生短暂的自动亮屏事件，因此需要合理设置候选时间窗和目标时间。
- 当前版本提供提醒命令，但不自动配置系统调度。

## License

[MIT](LICENSE)
