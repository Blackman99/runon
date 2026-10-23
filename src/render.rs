//! Human and JSON output for a scan.

use crate::scan::{Group, Report, Verdict};
use crate::util::{Paint, age, bytes, clip, iso, short, tilde};
use serde_json::{Value, json};

pub fn width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.parse::<usize>().ok())
        .filter(|w| *w >= 60)
        .unwrap_or(100)
        .min(160)
}

pub fn badge(c: &Paint, v: Verdict) -> String {
    let w = format!(" {:<8} ", v.word());
    match v {
        Verdict::Outlived => c.outlived(&w),
        Verdict::Returned => c.returned(&w),
        Verdict::Live => c.live(&w),
    }
}

pub fn text(r: &Report, all: bool, c: &Paint) -> String {
    let w = width();
    let strays: Vec<&Group> = r
        .groups
        .iter()
        .filter(|g| g.verdict != Verdict::Live)
        .collect();
    let live: Vec<&Group> = r
        .groups
        .iter()
        .filter(|g| g.verdict == Verdict::Live)
        .collect();
    let mut o = String::new();
    let head = match (strays.len(), r.stamped) {
        (0, 0) => "no process started by Claude Code, Codex or OpenCode is running".to_string(),
        (0, _) => format!(
            "nothing outlived its agent · {} agent-started {} still attached",
            r.stamped,
            if r.stamped == 1 {
                "process is"
            } else {
                "processes are"
            }
        ),
        (1, _) => "1 still running after its agent let go".to_string(),
        (n, _) => format!("{n} still running after their agent let go"),
    };
    let headc = if strays.is_empty() {
        c.green(&head)
    } else {
        c.bold(&head)
    };
    o += &format!("{}  {}\n", c.bold("runon"), headc);
    o += &c.dim(&format!(
        "       scanned {} processes in {:.1}s · {} started by agents · {} {} running",
        r.snap.procs.len(),
        r.snap.took.as_secs_f32(),
        r.stamped,
        r.harnesses.len(),
        if r.harnesses.len() == 1 {
            "harness"
        } else {
            "harnesses"
        },
    ));
    o += "\n";
    if r.stamped == 0 {
        o += &c.dim("       to see what a leak looks like: runon demo · to stop leaks at session end: runon hook claude");
        o += "\n";
    }
    let shown: Vec<&Group> = if all {
        r.groups.iter().collect()
    } else {
        strays.clone()
    };
    for g in &shown {
        o += "\n";
        o += &group(r, g, c, w);
    }
    if !all && !live.is_empty() {
        let n: usize = live.iter().map(|g| g.pids.len()).sum();
        o += &format!(
            "\n{}\n",
            c.dim(&format!(
                "  + {} live {} ({} {}) attached to running agents · runon --all",
                live.len(),
                if live.len() == 1 { "tree" } else { "trees" },
                n,
                if n == 1 { "process" } else { "processes" }
            ))
        );
    }
    if !strays.is_empty() {
        let has_ret = strays.iter().any(|g| g.verdict == Verdict::Returned);
        let has_out = strays.iter().any(|g| g.verdict == Verdict::Outlived);
        o += "\n";
        if has_out {
            o += &format!(
                "  stop exactly these   {}   then   {}\n",
                c.bold("runon stop --dry-run"),
                c.bold("runon stop")
            );
        }
        if has_ret {
            o += &c.dim("  RETURNED trees are left alone unless you add --returned (Codex may still be talking to them)\n");
        }
        if strays.iter().any(|g| g.demo) {
            o += &c.dim("  DEMO trees came from `runon demo` · end them with runon stop --demo\n");
        }
    }
    o
}

fn group(r: &Report, g: &Group, c: &Paint, w: usize) -> String {
    let s = &r.snap;
    let root = &s.procs[&g.root];
    let pad = "           ";
    let mut right = format!("pid {}", g.root);
    if g.pids.len() > 1 {
        right += &format!(" +{}", g.pids.len() - 1);
    }
    let tags: String = g.tags.iter().map(|t| format!(" {t}")).collect();
    let room = w.saturating_sub(11 + right.len() + tags.len() + 3).max(20);
    let cmd = clip(&root.command(), room);
    let gap = room.saturating_sub(cmd.chars().count()) + 2;
    let tagc = if g.tags.contains(&"HOT") {
        c.red(&tags)
    } else {
        c.ochre(&tags)
    };
    let mut o = format!(
        "{} {}{}{}  {}\n",
        badge(c, g.verdict),
        c.bold(&cmd),
        " ".repeat(gap),
        tagc,
        c.dim(&right)
    );

    let who = format!(
        "{} {}",
        g.stamp.agent.name(),
        g.stamp
            .session
            .as_deref()
            .map(short)
            .unwrap_or_else(|| "(no session id)".into())
    );
    let agent = g.stamp.agent.name();
    let why = match g.verdict {
        Verdict::Outlived => match g.session_harness {
            Some(h) => {
                format!("session still open ({agent} pid {h}) but no longer holds this")
            }
            None => {
                let mut x = "session is over".to_string();
                if let Some(t) = g.session.last_write {
                    x += &format!(" · log last written {} ago", age(s.now.saturating_sub(t)));
                }
                x
            }
        },
        Verdict::Returned => {
            let t = g
                .origin
                .as_ref()
                .and_then(|o| o.returned_at)
                .map(|t| age(s.now.saturating_sub(t)));
            match t {
                Some(t) => format!(
                    "tool call returned {t} ago · {agent} pid {} still open",
                    g.harness_pid.unwrap_or(0)
                ),
                None => format!(
                    "tool call returned · {agent} pid {} still open",
                    g.harness_pid.unwrap_or(0)
                ),
            }
        }
        Verdict::Live => format!("attached to {agent} pid {}", g.harness_pid.unwrap_or(0)),
    };
    o += &format!(
        "{pad}{}  {}\n",
        c.bold(&who),
        wrap(
            &why,
            w.saturating_sub(pad.len() + who.len() + 2),
            &" ".repeat(pad.len() + who.chars().count() + 2)
        )
    );

    match &g.origin {
        Some(or) => {
            let when = or
                .at
                .map(|t| format!(" {} ago", age(s.now.saturating_sub(t))))
                .unwrap_or_default();
            let bg = if or.background { " (background)" } else { "" };
            let id = or
                .call_id
                .as_deref()
                .map(|i| format!(" {}", clip(i, 14)))
                .unwrap_or_default();
            let lead = format!("{}{}{}{}: ", or.tool, bg, id, when);
            let room = w.saturating_sub(pad.len() + lead.len()).max(16);
            o += &format!(
                "{pad}{}{}\n",
                c.dim(&lead),
                clip(&or.command.replace('\n', " ⏎ "), room)
            );
        }
        None if g.tags.contains(&"MCP") => {
            o += &format!(
                "{pad}{}\n",
                c.dim(&format!(
                    "MCP server started by {agent} itself, not by a tool call"
                ))
            );
        }
        None if g.session.transcript.is_some() => {
            o += &format!(
                "{pad}{}\n",
                c.dim("no matching tool call in the session log")
            );
        }
        None => {
            if let Some(call) = &g.stamp.call {
                o += &format!("{pad}{}\n", c.dim(&format!("tool call {call}")));
            }
        }
    }

    let mut stats = vec![
        format!("up {}", age(s.now.saturating_sub(g.start))),
        format!("{:.1}% cpu", g.cpu),
        bytes(g.rss),
    ];
    if let Some(cwd) = root.cwd.as_deref().filter(|c| !c.is_empty()) {
        stats.push(tilde(cwd));
    } else if let Some(p) = &g.session.project {
        stats.push(p.clone());
    }
    o += &format!(
        "{pad}{}\n",
        c.dim(&clip(&stats.join(" · "), w.saturating_sub(pad.len())))
    );
    o
}

/// Greedy word wrap; continuation lines get `indent`.
fn wrap(s: &str, w: usize, indent: &str) -> String {
    let w = w.max(24);
    let mut out = String::new();
    let mut line = 0;
    for word in s.split(' ') {
        let n = word.chars().count();
        if line > 0 && line + 1 + n > w {
            out.push('\n');
            out.push_str(indent);
            line = 0;
        } else if line > 0 {
            out.push(' ');
            line += 1;
        }
        out.push_str(word);
        line += n;
    }
    out
}

pub fn group_json(r: &Report, g: &Group) -> Value {
    let s = &r.snap;
    let root = &s.procs[&g.root];
    json!({
        "verdict": g.verdict.word(),
        "agent": g.stamp.agent.name(),
        "session": g.stamp.session,
        "tool_call": g.stamp.call,
        "root_pid": g.root,
        "pids": g.pids,
        "processes": g.pids.iter().filter_map(|p| s.get(*p)).map(|p| json!({
            "pid": p.pid, "ppid": p.ppid, "command": p.command(), "cpu": p.cpu, "rss_bytes": p.rss, "started": iso(p.start),
        })).collect::<Vec<_>>(),
        "command": root.command(),
        "cwd": root.cwd,
        "started": iso(g.start),
        "age_secs": s.now.saturating_sub(g.start),
        "cpu": (g.cpu * 10.0).round() / 10.0,
        "rss_bytes": g.rss,
        "tags": g.tags,
        "harness_pid": g.harness_pid,
        "session_harness_pid": g.session_harness,
        "session_log": g.session.transcript,
        "session_log_last_write": g.session.last_write.map(iso),
        "origin": g.origin.as_ref().map(|o| json!({
            "tool": o.tool, "call_id": o.call_id, "command": o.command,
            "at": o.at.map(iso), "returned_at": o.returned_at.map(iso), "background": o.background,
        })),
    })
}

pub fn json(r: &Report, all: bool) -> Value {
    let groups: Vec<Value> = r
        .groups
        .iter()
        .filter(|g| all || g.verdict != Verdict::Live)
        .map(|g| group_json(r, g))
        .collect();
    let count = |v: Verdict| r.groups.iter().filter(|g| g.verdict == v).count();
    json!({
        "runon": env!("CARGO_PKG_VERSION"),
        "scanned": r.snap.procs.len(),
        "agent_processes": r.stamped,
        "elapsed_ms": r.snap.took.as_millis() as u64,
        "harnesses": r.harnesses.iter().map(|(p, a)| json!({"pid": p, "agent": a.name()})).collect::<Vec<_>>(),
        "counts": {"outlived": count(Verdict::Outlived), "returned": count(Verdict::Returned), "live": count(Verdict::Live)},
        "groups": groups,
    })
}
