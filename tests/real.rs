//! End to end against real OS processes: `runon demo` starts fake harnesses
//! (copies of runon named claude / codex) that stamp and spawn real children,
//! then lose track of some of them. No mocks of the process table.

use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

fn runon(args: &[&str], stdin: Option<&str>) -> (i32, Value, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_runon"));
    c.args(args)
        .env("NO_COLOR", "1")
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("CODEX_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut ch = c.spawn().expect("spawn runon");
    if let Some(s) = stdin {
        ch.stdin.as_mut().unwrap().write_all(s.as_bytes()).unwrap();
    }
    drop(ch.stdin.take());
    let out = ch.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let v = serde_json::from_str(&text).unwrap_or(Value::Null);
    (
        out.status.code().unwrap_or(-1),
        v,
        text + &String::from_utf8_lossy(&out.stderr),
    )
}

fn mine<'a>(v: &'a Value, sessions: &[String]) -> Vec<&'a Value> {
    v["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| {
            g["session"]
                .as_str()
                .is_some_and(|s| sessions.iter().any(|x| x == s))
        })
        .collect()
}

fn find<'a>(gs: &[&'a Value], cmd_start: &str) -> &'a Value {
    gs.iter()
        .find(|g| g["command"].as_str().unwrap().starts_with(cmd_start))
        .unwrap_or_else(|| panic!("no group for {cmd_start}: {gs:#?}"))
}

#[test]
fn demo_scan_hook_and_stop_on_real_processes() {
    let (code, v, raw) = runon(&["demo", "--json"], None);
    assert_eq!(code, 0, "{raw}");
    let sessions: Vec<String> = v["demo"]["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().into())
        .collect();
    let (a, b, c) = (&sessions[0], &sessions[1], &sessions[2]);
    let gs = mine(&v, &sessions);

    // session A ended: npm run dev and its vite child are still running
    let npm = find(&gs, "npm run dev");
    assert_eq!(npm["verdict"], "OUTLIVED", "{npm:#}");
    assert_eq!(npm["session"], a.as_str());
    assert_eq!(npm["pids"].as_array().unwrap().len(), 2, "npm + vite");
    assert_eq!(npm["session_harness_pid"], Value::Null);
    assert_eq!(npm["origin"]["tool"], "Bash");
    assert_eq!(npm["origin"]["command"], "npm run dev -- --port 5173");

    // session C is alive, but the shell above tsc was killed
    let tsc = find(&gs, "tsc --watch");
    assert_eq!(tsc["verdict"], "OUTLIVED", "{tsc:#}");
    assert!(
        tsc["session_harness_pid"].is_u64(),
        "session C's claude is still running"
    );

    // codex: the pytest call returned, the process did not
    let py = find(&gs, "pytest");
    assert_eq!(py["verdict"], "RETURNED", "{py:#}");
    assert_eq!(py["tool_call"], "call_demo_pytest");
    assert_eq!(py["session"], b.as_str());

    let watch = find(&gs, "cargo watch");
    assert_eq!(watch["verdict"], "LIVE");
    let mcp = find(&gs, "github-mcp");
    assert_eq!(mcp["verdict"], "LIVE");
    assert!(mcp["tags"].as_array().unwrap().iter().any(|t| t == "MCP"));

    // /clear must not touch anything
    let (code, _, raw) = runon(
        &["stop", "--hook"],
        Some(&format!(r#"{{"session_id":"{c}","reason":"clear"}}"#)),
    );
    assert_eq!(code, 0, "{raw}");
    assert_eq!(raw.trim(), "", "clear is a silent no-op");

    // SessionEnd for C: stops tsc and the MCP server, nothing from A or B
    let (code, v2, raw) = runon(
        &["stop", "--hook", "--json"],
        Some(&format!(
            r#"{{"session_id":"{c}","reason":"prompt_input_exit"}}"#
        )),
    );
    assert_eq!(code, 0, "{raw}");
    let stopped = v2["stopped"].as_array().unwrap();
    assert_eq!(stopped.len(), 2, "{raw}");
    assert!(stopped.iter().all(|s| s["result"] == "term"), "{raw}");

    let (_, v3, _) = runon(&["--json", "--all"], None);
    let left = mine(&v3, &sessions);
    assert!(
        left.iter().all(|g| g["session"] != c.as_str()),
        "C is clean"
    );
    assert_eq!(
        find(&left, "npm run dev")["verdict"],
        "OUTLIVED",
        "A untouched"
    );
    assert_eq!(find(&left, "pytest")["verdict"], "RETURNED", "B untouched");

    // without a terminal and without --yes, stop refuses before signalling
    let (code, _, raw) = runon(&["stop"], None);
    assert_eq!(code, 1, "{raw}");
    let (_, v4, _) = runon(&["--json"], None);
    assert!(
        !mine(&v4, &sessions).is_empty(),
        "nothing was stopped by the refused run"
    );

    // default stop takes OUTLIVED only; RETURNED needs --returned
    let (code, v5, raw) = runon(&["stop", "--yes", "--json"], None);
    assert_eq!(code, 0, "{raw}");
    let labels: Vec<&str> = v5["stopped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["label"].as_str().unwrap())
        .collect();
    assert!(labels.iter().any(|l| l.starts_with("npm run dev")), "{raw}");
    assert!(!labels.iter().any(|l| l.starts_with("pytest")), "{raw}");

    // and the demo cleans up after itself
    let (code, _, raw) = runon(&["stop", "--demo", "--json"], None);
    assert_eq!(code, 0, "{raw}");
    let (code, v6, raw) = runon(&["--json", "--all"], None);
    assert!(mine(&v6, &sessions).is_empty(), "{raw}");
    assert!(v6["harnesses"].as_array().unwrap().is_empty() || code == 0 || code == 3);
}

#[test]
fn usage_errors_exit_64() {
    assert_eq!(runon(&["--bogus"], None).0, 64);
    assert_eq!(runon(&["--dry-run"], None).0, 64, "stop-only flag on scan");
    assert_eq!(runon(&["stop", "--pid", "x"], None).0, 64);
    assert_eq!(runon(&["hook", "vim"], None).0, 64);
    let (code, _, out) = runon(&["hook", "claude"], None);
    assert_eq!(code, 0);
    assert!(out.contains("runon stop --hook"));
    let (code, _, out) = runon(&["--version"], None);
    assert_eq!(
        (code, out.trim()),
        (0, concat!("runon ", env!("CARGO_PKG_VERSION")))
    );
}
