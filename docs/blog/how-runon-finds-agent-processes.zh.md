# 编码代理如何标记它启动的进程，以及这些进程为何比会话活得更久

Claude Code、Codex CLI 和 OpenCode 都会替你执行 shell 命令。有时工具调用结束、点了停止、甚至整个会话结束后，命令仍在运行。anthropics/claude-code#104388 记录了一个例子：shell 被停止后，一个 `du` 又扫了大约 70 分钟磁盘。本文说明 runon 如何找到这类进程，以及如何判定它们的状态。

## 各个 harness 如何标记子进程

每个 harness 都会往它启动的命令的环境变量里写入标记。子进程继承父进程的环境，进程变成孤儿后环境变量依然保留，所以这些变量是可靠的标签。

| Harness | 变量 | 核对来源 |
|---|---|---|
| Codex | `CODEX_SESSION_ID`、`CODEX_THREAD_ID`、`CODEX_TOOL_CALL_ID` | `codex-rs/protocol/src/shell_environment.rs` |
| Claude Code 2.1.x | `CLAUDECODE=1`、`CLAUDE_CODE_SESSION_ID`（stdio MCP 服务器另有 `CLAUDE_PROJECT_DIR`） | 发行二进制中的字符串 |
| OpenCode | `OPENCODE=1`、`OPENCODE_PID`（1.19.x）或 `OPENCODE_SESSION_ID` | `packages/core/src/tool/plugin/shell.ts` |

runon 读取它有权限读取的每个进程的环境，只保留带有这些标记的进程。没有标记的进程不会被列出，也不会被停止，除非你用 `--pid` 明确指定。

## 为什么它们会比会话活得久

停止一次工具调用，通常只会杀掉 harness 启动的那个 shell，而不是它下面的所有进程。后台任务、管道中的某个命令、或者自行脱离的服务器会继续运行。在 Linux 和 macOS 上，它会被重新挂到 init 或 subreaper 下；在 Windows 上则根本不会重新挂靠，只保留旧的父 PID，而这个 PID 之后可能属于一个无关的进程。会话结束也不会改变这一点：没有任何东西去遍历进程树并发送信号。

## 判定是怎么做出的

对每个带标记的进程，runon 沿父进程链向上走。每一步都检查父进程的启动时间不晚于子进程，因为启动时间更晚的"父进程"说明 PID 已被复用，并不是真正的父进程。如果上方找到了存活的 harness 进程，就说明它仍然挂在会话上。

- **OUTLIVED**：上方已没有存活的 harness。会话已结束，或者它上方的 shell 已被杀掉。
- **RETURNED**（仅 Codex）：harness 仍然存活，但 `~/.codex/sessions/` 下的 rollout 中已经有启动该进程的 `CODEX_TOOL_CALL_ID` 对应的 `function_call_output`。
- **LIVE**：仍挂在会话上，且没有证据表明调用已结束。只在 `--all` 时显示。

`runon stop` 默认只处理 OUTLIVED 的进程树，每次发送信号前都会重新核对 PID 和启动时间，从不向 harness 进程发信号；没有终端时，除非加 `--yes`，否则什么都不停止。

## 测试了什么，没测试什么

CI 在 Linux、macOS 和 Windows 上，针对 `runon demo` 启动的真实进程运行测试。demo harness 是 runon 的一个副本，设置了相同的变量，并不是真正的 Claude Code、Codex 或 OpenCode。这些标记来自源码和二进制文件。此后做过一次真实验证：Linux 上的 OpenCode 1.19.4，实际标记是 `OPENCODE_PID` 而不是 `OPENCODE_SESSION_ID`。Claude Code、Codex 以及 macOS、Windows 尚未在真实会话中验证。如果判定看起来不对，在 issue 中附上 `runon --all --json` 的输出是最有用的报告。

[English](./how-runon-finds-agent-processes.md)
