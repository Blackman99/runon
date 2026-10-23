//! `runon stop`: end exactly the trees a scan found, nothing else.

use crate::procs::{Sig, alive, signal};
use crate::render::badge;
use crate::scan::{Group, Report, Verdict, harness};
use crate::util::{Paint, clip};
use serde_json::json;
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct StopOpts {
    pub returned: bool,
    pub sessions: Vec<String>,
    pub pids: Vec<u32>,
    pub dry_run: bool,
    pub yes: bool,
    pub grace: Duration,
    pub demo: bool,
    pub hook: Option<String>,
    pub json: bool,
}

pub struct Target {
    pub label: String,
    pub verdict: Option<Verdict>,
    /// (pid, start), root first
    pub procs: Vec<(u32, u64)>,
}

/// Pick what to stop. Never a harness, never runon or its ancestors, never an
/// unstamped process unless named with --pid.
pub fn select(r: &Report, o: &StopOpts) -> Result<Vec<Target>, String> {
    let s = &r.snap;
    let mine: HashSet<u32> = s.lineage(s.self_pid).into_iter().collect();
    let sess_match = |g: &Group, want: &str| {
        g.stamp
            .session
            .as_deref()
            .is_some_and(|x| x.starts_with(want))
    };
    let mut picked: Vec<&Group> = vec![];
    let mut out = vec![];
    let mut late = vec![];
    if let Some(sid) = &o.hook {
        // SessionEnd: everything this session started, attached or not.
        picked.extend(
            r.groups
                .iter()
                .filter(|g| g.stamp.session.as_deref() == Some(sid.as_str())),
        );
    } else if o.demo {
        picked.extend(r.groups.iter().filter(|g| g.demo));
        // the fake harnesses `runon demo` started
        let mut hs: Vec<_> = s
            .procs
            .values()
            .filter(|p| {
                p.env("RUNON_DEMO").is_some() && harness(p).is_some() && !mine.contains(&p.pid)
            })
            .collect();
        hs.sort_by_key(|p| p.pid);
        for p in hs {
            late.push(Target {
                label: format!("{} (demo harness)", p.name),
                verdict: None,
                procs: vec![(p.pid, p.start)],
            });
        }
    } else if !o.pids.is_empty() || !o.sessions.is_empty() {
        for want in &o.sessions {
            let n = picked.len();
            picked.extend(r.groups.iter().filter(|g| sess_match(g, want)));
            if picked.len() == n {
                return Err(format!(
                    "no agent-started process belongs to a session starting with `{want}`; `runon --all` lists every session id it can see"
                ));
            }
        }
        let kids = s.children();
        for &pid in &o.pids {
            if let Some(g) = r.groups.iter().find(|g| g.pids.contains(&pid)) {
                if g.root == pid {
                    picked.push(g);
                    continue;
                }
            }
            let Some(p) = s.get(pid) else {
                return Err(format!(
                    "no process with pid {pid} (it may have exited already; `runon --all` shows current pids)"
                ));
            };
            if mine.contains(&pid) {
                return Err(format!("pid {pid} is runon itself or one of its parents"));
            }
            if let Some(a) = harness(p) {
                return Err(format!(
                    "pid {pid} is a {} harness; runon does not stop agents themselves",
                    a.name()
                ));
            }
            let mut procs = vec![];
            let mut stack = vec![pid];
            while let Some(x) = stack.pop() {
                let Some(px) = s.get(x) else { continue };
                if procs.iter().any(|(q, _)| *q == x) || mine.contains(&x) || harness(px).is_some()
                {
                    continue;
                }
                procs.push((x, px.start));
                if let Some(ch) = kids.get(&x) {
                    stack.extend(ch.iter().rev());
                }
            }
            out.push(Target {
                label: clip(&p.command(), 60),
                verdict: None,
                procs,
            });
        }
    } else {
        picked.extend(r.groups.iter().filter(|g| {
            g.verdict == Verdict::Outlived || (o.returned && g.verdict == Verdict::Returned)
        }));
    }
    let mut seen = HashSet::new();
    for g in picked {
        if !seen.insert(g.root) {
            continue;
        }
        let procs = g
            .pids
            .iter()
            .filter(|p| !mine.contains(p))
            .filter_map(|p| s.get(*p).map(|x| (x.pid, x.start)))
            .collect::<Vec<_>>();
        if procs.is_empty() {
            continue;
        }
        out.push(Target {
            label: clip(&s.procs[&g.root].command(), 60),
            verdict: Some(g.verdict),
            procs,
        });
    }
    out.extend(late);
    Ok(out)
}

pub enum Outcome {
    Term,
    Kill,
    Gone,
    Failed(String),
}

/// TERM every process in the tree (deepest first), wait up to `grace`, then KILL
/// what is left. Each pid is re-checked against its start time right before
/// each signal so a recycled pid is never hit.
pub fn stop(t: &Target, grace: Duration) -> Outcome {
    let live: Vec<(u32, u64)> = t
        .procs
        .iter()
        .copied()
        .filter(|(p, st)| alive(*p, *st))
        .collect();
    if live.is_empty() {
        return Outcome::Gone;
    }
    for (p, st) in live.iter().rev() {
        if alive(*p, *st) {
            if let Err(e) = signal(*p, Sig::Term) {
                return Outcome::Failed(format!("pid {p}: {e}"));
            }
        }
    }
    let t0 = Instant::now();
    while t0.elapsed() < grace {
        if live.iter().all(|(p, st)| !alive(*p, *st)) {
            return Outcome::Term;
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    let mut failed = None;
    for (p, st) in live.iter().rev() {
        if alive(*p, *st) {
            if let Err(e) = signal(*p, Sig::Kill) {
                failed = Some(format!("pid {p}: {e}"));
            }
        }
    }
    let t1 = Instant::now();
    while t1.elapsed() < Duration::from_secs(2) {
        if live.iter().all(|(p, st)| !alive(*p, *st)) {
            return Outcome::Kill;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let left: Vec<String> = live
        .iter()
        .filter(|(p, st)| alive(*p, *st))
        .map(|(p, _)| p.to_string())
        .collect();
    Outcome::Failed(
        failed.unwrap_or_else(|| format!("still running after KILL: {}", left.join(", "))),
    )
}

pub fn run(r: &Report, o: &StopOpts, c: &Paint) -> Result<i32, String> {
    let targets = select(r, o)?;
    let n: usize = targets.iter().map(|t| t.procs.len()).sum();
    if targets.is_empty() {
        if o.json {
            println!("{}", json!({"stopped": [], "dry_run": o.dry_run}));
        } else if o.hook.is_none() {
            let extra = if !o.returned && r.groups.iter().any(|g| g.verdict == Verdict::Returned) {
                " (RETURNED trees exist: add --returned)"
            } else {
                ""
            };
            let what = if o.demo {
                "no `runon demo` processes are left"
            } else {
                "nothing to stop"
            };
            println!("{}  {what}{extra}", c.bold("runon"));
        }
        return Ok(0);
    }
    if !o.json {
        let verb = if o.dry_run || (!o.yes && o.hook.is_none() && !o.demo) {
            "would stop"
        } else {
            "stopping"
        };
        println!(
            "{}  {verb} {} {} · {} {}",
            c.bold("runon"),
            targets.len(),
            if targets.len() == 1 { "tree" } else { "trees" },
            n,
            if n == 1 { "process" } else { "processes" }
        );
        for t in &targets {
            let b = t
                .verdict
                .map(|v| badge(c, v))
                .unwrap_or_else(|| c.dim(if o.demo { " HARNESS  " } else { " BY PID   " }));
            let pids: Vec<String> = t.procs.iter().map(|p| p.0.to_string()).collect();
            println!(
                " {b} {}  {}",
                t.label,
                c.dim(&format!("pid {}", pids.join(" ")))
            );
        }
    }
    if o.dry_run {
        if o.json {
            let v: Vec<_> = targets.iter().map(|t| json!({"label": t.label, "pids": t.procs.iter().map(|p| p.0).collect::<Vec<_>>()})).collect();
            println!("{}", json!({"dry_run": true, "would_stop": v}));
        } else {
            println!(
                "{}",
                c.dim("  dry run: nothing was signalled · drop --dry-run to stop these")
            );
        }
        return Ok(0);
    }
    if !o.yes && o.hook.is_none() && !o.demo {
        use std::io::{IsTerminal, Write};
        if !std::io::stdin().is_terminal() {
            return Err(
                "no terminal to ask on, so nothing was stopped; re-run with --yes to stop the trees above, or --dry-run to only list them".into(),
            );
        }
        print!("  stop them? [y/N] ");
        let _ = std::io::stdout().flush();
        let mut a = String::new();
        let _ = std::io::stdin().read_line(&mut a);
        if !matches!(a.trim(), "y" | "Y" | "yes") {
            println!("{}", c.dim("  left everything running"));
            return Ok(0);
        }
    }
    let mut results = vec![];
    let mut bad = 0;
    for t in &targets {
        let out = stop(t, o.grace);
        let (word, detail) = match &out {
            Outcome::Term => (c.green("stopped"), "exited on TERM".to_string()),
            Outcome::Kill => (
                c.ochre("killed "),
                format!("ignored TERM for {}s, KILL", o.grace.as_secs()),
            ),
            Outcome::Gone => (c.dim("gone   "), "already exited".to_string()),
            Outcome::Failed(e) => {
                bad += 1;
                (c.red("FAILED "), e.clone())
            }
        };
        if !o.json {
            let pids: Vec<String> = t.procs.iter().map(|p| p.0.to_string()).collect();
            println!(
                "  {word} {}  {}",
                t.label,
                c.dim(&format!("pid {} · {detail}", pids.join(" ")))
            );
        }
        results.push(json!({
            "label": t.label,
            "pids": t.procs.iter().map(|p| p.0).collect::<Vec<_>>(),
            "result": match out { Outcome::Term => "term", Outcome::Kill => "kill", Outcome::Gone => "gone", Outcome::Failed(_) => "failed" },
            "detail": detail,
        }));
    }
    if o.demo {
        remove_demo_dirs(r);
    }
    if o.json {
        println!("{}", json!({"stopped": results}));
    }
    Ok(if bad > 0 { 1 } else { 0 })
}

/// `runon stop --demo` also deletes the demo's own temp folders (and only
/// folders named runon-demo-* inside the temp dir).
fn remove_demo_dirs(r: &Report) {
    let tmp = std::env::temp_dir();
    let mut dirs: Vec<String> = r
        .snap
        .procs
        .values()
        .filter_map(|p| p.env("RUNON_DEMO_HOME").map(String::from))
        .collect();
    dirs.sort();
    dirs.dedup();
    for d in dirs {
        let p = std::path::Path::new(&d);
        let named = p
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("runon-demo-"));
        if named && p.parent() == Some(tmp.as_path()) {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}
