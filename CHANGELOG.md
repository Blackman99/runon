# Changelog

## Unreleased

- OpenCode: read `OPENCODE_PID`, which OpenCode 1.19.x sets instead of `OPENCODE_SESSION_ID`, to link an orphan to its still-running session (with a start-time check against PID reuse).
- Checked in a real OpenCode 1.19.4 session on Linux. Claude Code and Codex, and macOS/Windows, are still unverified in real sessions.

## 0.1.0 — 2026-10-23

First release.

- `runon` scans the process table and lists processes started by Claude Code, Codex and OpenCode sessions (found through the env stamps each harness sets), with a verdict for each: OUTLIVED (its session is gone), RETURNED (the tool call that started it has finished, but it is still running) or LIVE.
- Tags: HOT (high CPU), BIG (high memory), MCP (a stdio MCP server).
- `runon stop` sends TERM, waits `--grace` seconds, then sends KILL. Takes a session, a pid, `--outlived` or `--demo`, with `--dry-run` and `--yes`.
- `runon hook claude|codex|opencode` prints a session-end hook that stops what the session left behind.
- `runon demo` starts 3 fake sessions with real child processes, so you can see the output without an agent.
- `--json`, plus exit codes 0 (clean), 3 (strays found), 1 (error) and 64 (usage).
- Prebuilt binaries for Linux (musl), macOS and Windows on x86_64 and aarch64, plus install.sh, which checks sha256.

Not yet tested inside a real harness session. The env stamps were checked against each harness's source and docs, and the demo uses the same stamps.
