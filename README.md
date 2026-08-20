# Screen Wake Clock

一个面向 macOS 的 Rust 命令行工具。它从 `pmset -g log` 中读取屏幕亮起事件，估算当天的大致打卡时间和下班时间，并把结果保存为每日 JSON 记录。

安装一次后，工具通过两个独立的 macOS launchd 任务自动工作：每天只计算并写入一次记录；提醒检查不会重新计算或覆盖当天记录。

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

需要 macOS 和 Rust 工具链。使用 curl 一键完成下载、编译、配置和自动启动：

```bash
curl -fsSL https://raw.githubusercontent.com/weierz/screen-wake-clock/main/install.sh | zsh
```

也可以从克隆的仓库安装：

```bash
git clone https://github.com/weierz/screen-wake-clock.git
cd screen-wake-clock
./install.sh
```

安装脚本会完成编译、安装并立即加载两个用户级 LaunchAgent。用户不需要再执行启动命令，也不需要管理员权限。

### Homebrew

通过 Homebrew tap 安装并启动服务：

```bash
brew install weierz/tap/wake-clock && brew services start weierz/tap/wake-clock
```

Homebrew 使用一个常驻的轻量服务读取同一份配置：每天只生成一次记录，并自动完成下班提醒。停止和卸载：

```bash
brew services stop weierz/tap/wake-clock
brew uninstall weierz/tap/wake-clock
```

## 日常使用

安装完成后不需要手动执行 `record` 或 `remind`：

- 默认每天 `14:05` 自动读取一次亮屏日志并保存记录。
- 每 60 秒进行一次轻量提醒检查，只在配置的提醒窗口内发送一次通知。
- 登录或电脑从关机状态恢复时，如果已经超过当天记录时间，会补做当天记录。
- 同一天已有记录时不会覆盖，除非明确使用 `--force`。

查看记录：

```bash
wake-clock status
wake-clock history
wake-clock history 30
```

卸载自动任务，但保留配置和历史记录：

```bash
wake-clock uninstall
```

以下命令只用于调试或手动修正：

```bash
wake-clock record           # 如果今天没有记录，则立即记录
wake-clock record --force   # 强制重新计算今天的记录
wake-clock remind           # 立即检查提醒状态
wake-clock init             # 创建默认配置
wake-clock help
```

## 配置

安装时会自动创建（也可通过 `init` 或 `record` 创建）：

```text
~/.config/wake-clock/config.toml
```

完整示例见 [`config.example.toml`](config.example.toml)。主要配置项：

- `search.target_time`：选择亮屏事件时的目标时间。
- `search.start_time` / `search.end_time`：候选事件时间窗。
- `schedule.max_end_time`：最晚下班时间。
- `schedule.reminder_minutes`：提前多少分钟提醒。
- `schedule.rules`：按顺序匹配的下班时间规则。
- `automation.daily_record_time`：每天自动记录时间，应晚于候选时间窗。
- `automation.reminder_check_seconds`：提醒检查间隔，最小 30 秒。

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

修改自动化时间或检查间隔后，重新执行一次：

```bash
wake-clock install
```

## 数据

每日记录保存在：

```text
~/.local/share/wake-clock/records/YYYY-MM-DD.json
```

后台任务日志位于：

```text
~/.local/share/wake-clock/wake-clock.log
~/.local/share/wake-clock/wake-clock.error.log
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
- macOS 必须允许 `osascript` 发送通知，否则提醒记录不会标记为已发送。

## License

[MIT](LICENSE)
