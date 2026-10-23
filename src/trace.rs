//! Provenance: find the tool call that started a process in the agent's own
//! session log. Claude Code: ~/.claude/projects/*/<session>.jsonl.
//! Codex: ~/.codex/sessions/**/rollout-*-<thread>.jsonl.

use crate::procs::{Proc, Snapshot};
use crate::scan::{Agent, Group, Origin, Report, SessionInfo, Verdict};
use crate::util::{parse_iso, tilde};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
pub struct Homes {
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
}

impl Homes {
    pub fn resolve(&self, agent: Agent, demo_home: Option<&str>) -> Option<PathBuf> {
        if let Some(d) = demo_home {
            return Some(Path::new(d).join(agent.name()));
        }
        match agent {
            Agent::Claude => self
                .claude
                .clone()
                .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
                .or_else(|| crate::util::home().map(|h| h.join(".claude"))),
            Agent::Codex => self
                .codex
                .clone()
                .or_else(|| std::env::var_os("CODEX_HOME").map(PathBuf::from))
                .or_else(|| crate::util::home().map(|h| h.join(".codex"))),
            Agent::OpenCode => None,
        }
    }
}

/// One parsed session log, cached per path.
#[derive(Default, Clone)]
struct Log {
    calls: Vec<Call>,
    project: Option<String>,
    last_write: Option<u64>,
}

#[derive(Clone, Debug)]
struct Call {
    id: String,
    tool: String,
    command: String,
    at: Option<u64>,
    returned_at: Option<u64>,
    background: bool,
}

pub fn annotate(r: &mut Report, homes: &Homes) {
    let mut cache: HashMap<PathBuf, Option<Log>> = HashMap::new();
    for g in &mut r.groups {
        let Some(sess) = g.stamp.session.clone() else {
            continue;
        };
        let root = &r.snap.procs[&g.root];
        let Some(home) = homes.resolve(g.stamp.agent, root.env("RUNON_DEMO_HOME")) else {
            continue;
        };
        let Some(path) = find_log(g.stamp.agent, &home, &sess) else {
            continue;
        };
        let log = cache
            .entry(path.clone())
            .or_insert_with(|| parse(g.stamp.agent, &path))
            .clone();
        let Some(log) = log else { continue };
        g.session = SessionInfo {
            transcript: Some(tilde(&path.to_string_lossy())),
            project: log.project.clone(),
            last_write: log.last_write,
        };
        g.origin = matched(g, &r.snap, &log).map(|c| Origin {
            tool: c.tool.clone(),
            call_id: Some(c.id.clone()),
            command: c.command.clone(),
            at: c.at,
            returned_at: c.returned_at,
            background: c.background,
        });
        if g.verdict == Verdict::Live
            && g.stamp.agent == Agent::Codex
            && g.origin.as_ref().is_some_and(|o| o.returned_at.is_some())
        {
            g.verdict = Verdict::Returned;
        }
    }
    r.groups.sort_by(|a, b| {
        a.verdict
            .cmp(&b.verdict)
            .then(a.start.cmp(&b.start))
            .then(a.root.cmp(&b.root))
    });
}

fn matched<'a>(g: &Group, s: &Snapshot, log: &'a Log) -> Option<&'a Call> {
    // Codex tells us the exact call id.
    if let Some(id) = &g.stamp.call {
        return log.calls.iter().find(|c| &c.id == id);
    }
    // Claude: the newest Bash call issued before the process started whose
    // command text shows up in the tree's command lines (or vice versa).
    let lines: Vec<String> = g
        .pids
        .iter()
        .filter_map(|p| s.get(*p))
        .map(norm_proc)
        .collect();
    let start = g.start;
    log.calls
        .iter()
        .rev()
        .filter(|c| c.at.is_none_or(|t| t <= start + 5))
        .find(|c| {
            let cmd = norm(&c.command);
            !cmd.is_empty()
                && lines
                    .iter()
                    .any(|l| l.contains(&cmd) || (l.len() >= 6 && cmd.contains(l.as_str())))
        })
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn norm_proc(p: &Proc) -> String {
    // drop the directory part of argv[0] so `npm run dev` matches `/usr/bin/npm run dev`
    let mut v = p.cmd.clone();
    if let Some(f) = v.first_mut() {
        let b = crate::procs::base(f).to_string();
        *f = b.strip_suffix(".exe").map(String::from).unwrap_or(b);
    }
    norm(&v.join(" "))
}

fn find_log(agent: Agent, home: &Path, session: &str) -> Option<PathBuf> {
    if session.contains(['/', '\\']) || session.contains("..") {
        return None;
    }
    match agent {
        Agent::Claude => {
            let projects = home.join("projects");
            for e in std::fs::read_dir(&projects).ok()?.flatten() {
                let f = e.path().join(format!("{session}.jsonl"));
                if f.is_file() {
                    return Some(f);
                }
            }
            None
        }
        Agent::Codex => {
            let suffix = format!("-{session}.jsonl");
            let mut stack = vec![(home.join("sessions"), 0)];
            let mut best: Option<PathBuf> = None;
            while let Some((d, depth)) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&d) else {
                    continue;
                };
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() && depth < 4 {
                        stack.push((p, depth + 1));
                    } else if p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(&suffix))
                    {
                        best = Some(p);
                    }
                }
            }
            best
        }
        Agent::OpenCode => None,
    }
}

/// Session logs can be hundreds of MB; only the last 32 MB are read.
const TAIL: u64 = 32 * 1024 * 1024;

fn parse(agent: Agent, path: &Path) -> Option<Log> {
    use std::io::{Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let meta = f.metadata().ok()?;
    let last_write = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let skip = meta.len().saturating_sub(TAIL);
    f.seek(SeekFrom::Start(skip)).ok()?;
    let mut rd = BufReader::new(f);
    if skip > 0 {
        let mut junk = Vec::new();
        rd.read_until(b'\n', &mut junk).ok()?;
    }
    let mut log = Log {
        last_write,
        ..Default::default()
    };
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut buf = String::new();
    loop {
        buf.clear();
        match rd.read_line(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if buf.len() > 4 * 1024 * 1024 {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&buf) else {
            continue;
        };
        let at = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_iso);
        match agent {
            Agent::Claude => claude_line(&v, at, &mut log, &mut by_id),
            Agent::Codex => codex_line(&v, at, &mut log, &mut by_id),
            Agent::OpenCode => {}
        }
    }
    Some(log)
}

fn claude_line(v: &Value, at: Option<u64>, log: &mut Log, by_id: &mut HashMap<String, usize>) {
    if let Some(c) = v.get("cwd").and_then(Value::as_str) {
        log.project = Some(tilde(c));
    }
    let Some(content) = v.pointer("/message/content").and_then(Value::as_array) else {
        return;
    };
    for item in content {
        match item.get("type").and_then(Value::as_str) {
            Some("tool_use") if item.get("name").and_then(Value::as_str) == Some("Bash") => {
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let command = item
                    .pointer("/input/command")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let background = item
                    .pointer("/input/run_in_background")
                    .and_then(Value::as_bool)
                    == Some(true);
                by_id.insert(id.clone(), log.calls.len());
                log.calls.push(Call {
                    id,
                    tool: "Bash".into(),
                    command,
                    at,
                    returned_at: None,
                    background,
                });
            }
            Some("tool_result") => {
                if let Some(i) = item
                    .get("tool_use_id")
                    .and_then(Value::as_str)
                    .and_then(|id| by_id.get(id))
                {
                    log.calls[*i].returned_at = at;
                }
            }
            _ => {}
        }
    }
}

fn codex_line(v: &Value, at: Option<u64>, log: &mut Log, by_id: &mut HashMap<String, usize>) {
    let Some(p) = v.get("payload") else { return };
    if let Some(c) = p.get("cwd").and_then(Value::as_str) {
        log.project = Some(tilde(c));
    }
    let ty = p.get("type").and_then(Value::as_str).unwrap_or_default();
    let call_id = p.get("call_id").and_then(Value::as_str).map(String::from);
    match ty {
        "function_call" | "custom_tool_call" | "local_shell_call" => {
            let Some(id) = call_id else { return };
            let tool = p
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(ty)
                .to_string();
            let command = codex_command(p);
            by_id.insert(id.clone(), log.calls.len());
            log.calls.push(Call {
                id,
                tool,
                command,
                at,
                returned_at: None,
                background: false,
            });
        }
        "function_call_output" | "custom_tool_call_output" | "exec_command_end" => {
            if let Some(i) = call_id.and_then(|id| by_id.get(&id).copied()) {
                log.calls[i].returned_at = log.calls[i].returned_at.or(at);
            }
        }
        _ => {}
    }
}

fn codex_command(p: &Value) -> String {
    let args = p.get("arguments").or(p.get("input")).or(p.get("action"));
    let parsed = match args {
        Some(Value::String(s)) => {
            serde_json::from_str::<Value>(s).unwrap_or(Value::String(s.clone()))
        }
        Some(v) => v.clone(),
        None => return String::new(),
    };
    for k in ["cmd", "command"] {
        match parsed.get(k) {
            Some(Value::String(s)) => return s.clone(),
            Some(Value::Array(a)) => {
                let parts: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
                // ["bash","-lc","the command"] -> the command
                if parts.len() == 3 && (parts[1] == "-lc" || parts[1] == "-c") {
                    return parts[2].to_string();
                }
                return parts.join(" ");
            }
            _ => {}
        }
    }
    match parsed {
        Value::String(s) => s,
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_rollout_marks_returned_calls() {
        let d = std::env::temp_dir().join(format!("runon-trace-{}", std::process::id()));
        let s = d.join("sessions/2026/10/23");
        std::fs::create_dir_all(&s).unwrap();
        let f = s.join("rollout-2026-10-23T10-00-00-019a-thread.jsonl");
        std::fs::write(
            &f,
            concat!(
                r#"{"timestamp":"2026-10-23T02:00:00Z","type":"session_meta","payload":{"id":"019a-thread","cwd":"/w/app"}}"#, "\n",
                r#"{"timestamp":"2026-10-23T02:00:05Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"call_1","arguments":"{\"cmd\":\"pytest -x\"}"}}"#, "\n",
                r#"{"timestamp":"2026-10-23T02:00:09Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_1","output":"..."}}"#, "\n",
                r#"{"timestamp":"2026-10-23T02:00:10Z","type":"response_item","payload":{"type":"function_call","name":"shell","call_id":"call_2","arguments":"{\"command\":[\"bash\",\"-lc\",\"cargo watch\"]}"}}"#, "\n",
            ),
        )
        .unwrap();
        let p = find_log(Agent::Codex, &d, "019a-thread").unwrap();
        let log = parse(Agent::Codex, &p).unwrap();
        assert_eq!(log.project.as_deref(), Some("/w/app"));
        assert_eq!(log.calls.len(), 2);
        assert_eq!(log.calls[0].command, "pytest -x");
        assert!(log.calls[0].returned_at.is_some());
        assert_eq!(log.calls[1].command, "cargo watch");
        assert!(log.calls[1].returned_at.is_none());
        assert!(find_log(Agent::Codex, &d, "../etc").is_none());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn claude_transcript_bash_calls() {
        let d = std::env::temp_dir().join(format!("runon-trace-c-{}", std::process::id()));
        let pdir = d.join("projects/-w-shop");
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(
            pdir.join("s1.jsonl"),
            concat!(
                r#"{"timestamp":"2026-10-23T02:00:00Z","cwd":"/w/shop","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"npm run dev","run_in_background":true}}]}}"#, "\n",
                r#"{"timestamp":"2026-10-23T02:00:01Z","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1"}]}}"#, "\n",
            ),
        )
        .unwrap();
        let log = parse(Agent::Claude, &find_log(Agent::Claude, &d, "s1").unwrap()).unwrap();
        assert_eq!(log.calls[0].command, "npm run dev");
        assert!(log.calls[0].background && log.calls[0].returned_at.is_some());
        let _ = std::fs::remove_dir_all(d);
    }
}
