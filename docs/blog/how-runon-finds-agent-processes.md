# How coding agents mark the processes they start, and why those processes outlive the session

Claude Code, Codex CLI and OpenCode all run shell commands for you. Sometimes the command keeps running after the tool call, the stop button, or the whole session is over. anthropics/claude-code#104388 describes one case: a `du` kept walking a disk for about 70 minutes after its shell was stopped. This article explains how runon finds such processes and how it decides what to call them.

## How the harnesses mark child processes

Each harness puts variables into the environment of the commands it spawns. A child inherits its parent's environment, and the environment stays with a process after it is orphaned, so these variables are a durable label.

| Harness | Variables | Where they were checked |
|---|---|---|
| Codex | `CODEX_SESSION_ID`, `CODEX_THREAD_ID`, `CODEX_TOOL_CALL_ID` | `codex-rs/protocol/src/shell_environment.rs` |
| Claude Code 2.1.x | `CLAUDECODE=1`, `CLAUDE_CODE_SESSION_ID` (+ `CLAUDE_PROJECT_DIR` for stdio MCP servers) | strings in the shipped binary |
| OpenCode | `OPENCODE=1`, `OPENCODE_SESSION_ID` | `packages/core/src/tool/plugin/shell.ts` |

runon reads the environment of every process it is allowed to read and keeps only those carrying one of these stamps. A process without a stamp is never listed and never stopped unless you name it with `--pid`.

## Why they outlive the session

Stopping a tool call usually kills the shell the harness started, not everything below it. A background job, a pipeline member, or a server that daemonized keeps running. On Linux and macOS it is reparented (to init or a subreaper); on Windows it is not reparented at all and simply keeps its old parent PID, which may later belong to an unrelated process. Ending the session does not change that: nothing walks the tree and signals it.

## How the verdicts are decided

For each stamped process runon walks up the parent chain. At every step it checks that the parent started no later than the child, because a parent PID that started later has been reused and is not the real parent. If a live harness process is found above it, the process is still attached.

- **OUTLIVED**: no live harness above it. The session ended, or the shell above it was killed.
- **RETURNED** (Codex only): the harness is still alive, but the session rollout under `~/.codex/sessions/` already contains the `function_call_output` for the `CODEX_TOOL_CALL_ID` that started the process.
- **LIVE**: attached and no evidence the call has finished. Hidden unless `--all`.

`runon stop` touches only OUTLIVED trees by default, re-checks PID and start time right before each signal, never signals a harness process, and without a terminal does nothing unless `--yes` is given.

## What was tested, and what was not

CI runs on Linux, macOS and Windows against real processes started by `runon demo`. The demo harness is a copy of runon that sets the same variables; it is not Claude Code, Codex or OpenCode. The stamps were taken from source and binaries. runon has not yet been checked inside a real session of any of the three harnesses. If a verdict looks wrong, the output of `runon --all --json` in an issue is the most useful report.

[中文版](./how-runon-finds-agent-processes.zh.md)
