mod demo;
mod procs;
mod render;
mod scan;
mod stop;
mod trace;
mod util;

use std::time::Duration;
use util::Paint;

const HELP: &str = "\
runon: find the processes your coding agents started that are still running
after the tool call, the stop, or the whole session ended. Stop exactly those.

USAGE
  runon [--all] [--json]          scan (default)
  runon stop [options]            stop OUTLIVED trees (asks first)
  runon demo                      start a small real mess and scan it
  runon hook claude               print a Claude Code SessionEnd hook
  runon version

SCAN
  --all            also list LIVE trees still attached to a running agent
  --json           machine-readable output
  --claude-home P  Claude Code config dir   (default $CLAUDE_CONFIG_DIR or ~/.claude)
  --codex-home P   Codex home               (default $CODEX_HOME or ~/.codex)
  --no-color       plain output (NO_COLOR is honoured too)

STOP
  --dry-run        show what would be stopped, signal nothing
  --yes            do not ask (needed without a terminal)
  --returned       also stop RETURNED trees (Codex call returned, process didn't)
  --session ID     everything one session started, attached or not (prefix ok)
  --pid N          this process and its children, even if unstamped
  --grace SECS     wait between TERM and KILL (default 3)
  --demo           end what `runon demo` started
  --hook           SessionEnd hook mode: read {session_id} JSON on stdin

EXIT
  0 nothing outlived its agent   3 strays found   1 error   64 usage
";

struct Args {
    cmd: String,
    all: bool,
    json: bool,
    no_color: bool,
    homes: trace::Homes,
    stop: stop::StopOpts,
    hook_kind: Option<String>,
}

fn parse(argv: Vec<String>) -> Result<Args, String> {
    let mut a = Args {
        cmd: "scan".into(),
        all: false,
        json: false,
        no_color: false,
        homes: Default::default(),
        stop: stop::StopOpts {
            grace: Duration::from_secs(3),
            ..Default::default()
        },
        hook_kind: None,
    };
    let mut it = argv.into_iter().peekable();
    if let Some(first) = it.peek() {
        if !first.starts_with('-') {
            let c = it.next().unwrap_or_default();
            a.cmd = match c.as_str() {
                "scan" | "stop" | "demo" | "hook" | "version" | "help" => c,
                other => return Err(format!("unknown command `{other}` (try runon --help)")),
            };
        }
    }
    let mut hook_flag = false;
    while let Some(x) = it.next() {
        let mut val = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match x.as_str() {
            "-h" | "--help" => a.cmd = "help".into(),
            "-V" | "--version" => a.cmd = "version".into(),
            "--all" | "-a" => a.all = true,
            "--json" => a.json = true,
            "--no-color" => a.no_color = true,
            "--claude-home" => a.homes.claude = Some(val("--claude-home")?.into()),
            "--codex-home" => a.homes.codex = Some(val("--codex-home")?.into()),
            "--dry-run" | "-n" => a.stop.dry_run = true,
            "--yes" | "-y" => a.stop.yes = true,
            "--returned" => a.stop.returned = true,
            "--session" => a.stop.sessions.push(val("--session")?),
            "--pid" => {
                let v = val("--pid")?;
                a.stop.pids.push(
                    v.parse()
                        .map_err(|_| format!("--pid wants a number, got `{v}`"))?,
                );
            }
            "--grace" => {
                let v = val("--grace")?;
                let s: f64 = v
                    .parse()
                    .map_err(|_| format!("--grace wants seconds, got `{v}`"))?;
                a.stop.grace = Duration::from_secs_f64(s.clamp(0.0, 600.0));
            }
            "--demo" => a.stop.demo = true,
            "--hook" => hook_flag = true,
            other if a.cmd == "hook" && !other.starts_with('-') && a.hook_kind.is_none() => {
                a.hook_kind = Some(other.to_string())
            }
            other => return Err(format!("unknown option `{other}` (try runon --help)")),
        }
    }
    if hook_flag {
        a.stop.hook = Some(String::new());
    }
    let stop_only = a.stop.dry_run
        || a.stop.yes
        || a.stop.returned
        || !a.stop.sessions.is_empty()
        || !a.stop.pids.is_empty()
        || a.stop.demo
        || hook_flag;
    if stop_only && a.cmd != "stop" {
        return Err(
            "--dry-run/--yes/--returned/--session/--pid/--demo/--hook belong to `runon stop`"
                .into(),
        );
    }
    Ok(a)
}

fn scan(homes: &trace::Homes) -> scan::Report {
    let mut r = scan::classify(procs::Snapshot::take());
    trace::annotate(&mut r, homes);
    r
}

fn main() {
    if let Some(code) = demo::role() {
        std::process::exit(code);
    }
    let a = match parse(std::env::args().skip(1).collect()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("runon: {e}");
            std::process::exit(64);
        }
    };
    let c = Paint::detect(a.no_color);
    let code = match a.cmd.as_str() {
        "help" => {
            print!("{HELP}");
            0
        }
        "version" => {
            println!("runon {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "hook" => hook(&a),
        "demo" => run_demo(&a, &c),
        "stop" => run_stop(a, &c),
        _ => {
            let r = scan(&a.homes);
            if a.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&render::json(&r, a.all)).unwrap_or_default()
                );
            } else {
                print!("{}", render::text(&r, a.all, &c));
            }
            exit_for(&r)
        }
    };
    std::process::exit(code);
}

fn exit_for(r: &scan::Report) -> i32 {
    if r.groups.iter().any(|g| g.verdict != scan::Verdict::Live) {
        3
    } else {
        0
    }
}

fn hook(a: &Args) -> i32 {
    match a.hook_kind.as_deref() {
        Some("claude") => {
            let v = serde_json::json!({"hooks": {"SessionEnd": [{"hooks": [{"type": "command", "command": "runon stop --hook", "timeout": 15}]}]}});
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            eprintln!(
                "\nMerge this into ~/.claude/settings.json (or .claude/settings.json in a repo).\n\
                 When a Claude Code session ends, runon stops what that session started\n\
                 (its session id comes in on stdin). /clear is ignored, so MCP servers survive it."
            );
            0
        }
        Some(other) => {
            eprintln!(
                "runon: no `{other}` hook yet. Claude Code only for now (Codex and OpenCode are on the roadmap)."
            );
            64
        }
        None => {
            eprintln!("runon: which agent? try `runon hook claude`");
            64
        }
    }
}

fn run_stop(mut a: Args, c: &Paint) -> i32 {
    if a.stop.hook.is_some() {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut s);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap_or_default();
        let sid = v
            .get("session_id")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        let reason = v.get("reason").and_then(|x| x.as_str()).unwrap_or_default();
        if sid.is_empty() {
            eprintln!("runon: --hook expects the hook JSON on stdin ({{\"session_id\": ...}})");
            return 1;
        }
        if reason == "clear" {
            return 0;
        }
        a.stop.hook = Some(sid);
    }
    a.stop.json = a.json;
    let r = scan(&a.homes);
    match stop::run(&r, &a.stop, c) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("runon: {e}");
            1
        }
    }
}

fn run_demo(a: &Args, c: &Paint) -> i32 {
    if !a.json {
        println!(
            "{}",
            c.dim("runon demo · starting 3 fake agent sessions with real child processes…")
        );
    }
    let d = match demo::start() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("runon: demo failed: {e}");
            return 1;
        }
    };
    // wait until the mess has settled (max ~6s)
    let t0 = std::time::Instant::now();
    let mut r = scan(&a.homes);
    loop {
        let mine = |v: scan::Verdict| {
            r.groups
                .iter()
                .filter(|g| {
                    g.demo
                        && g.verdict == v
                        && g.stamp
                            .session
                            .as_ref()
                            .is_some_and(|s| d.sessions.contains(s))
                })
                .count()
        };
        if (mine(scan::Verdict::Outlived) >= 2 && mine(scan::Verdict::Returned) >= 1)
            || t0.elapsed().as_secs() >= 6
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
        r = scan(&a.homes);
    }
    if a.json {
        let mut v = render::json(&r, true);
        v["demo"] = serde_json::json!({"dir": d.dir, "sessions": d.sessions});
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        print!("{}", render::text(&r, false, c));
        println!(
            "{}",
            c.dim(&format!(
                "\n  these are real processes. run `runon` again to see them, `runon stop --demo` to end them.\n  they exit by themselves in {} minutes. files: {}",
                demo::LIFETIME / 60,
                util::tilde(&d.dir.to_string_lossy())
            ))
        );
    }
    0
}
