//! `runon demo`: build a small, real agent mess on this machine and scan it.
//!
//! Copies of the runon binary named `claude` and `codex` play the harnesses;
//! they set the same env stamps the real ones do and start real child
//! processes (more copies of runon, renamed `npm`, `vite`, `tsc`, `pytest`,
//! `cargo`, `github-mcp`). Then:
//!   * claude session A exits while its `npm run dev` (+ `vite`) keep running,
//!   * claude session C hard-kills the shell running `tsc --watch` (what a
//!     stopped background task does), so `tsc` keeps running under nobody,
//!   * codex session B gets a finished `pytest` tool call whose process never left.
//!
//! Every demo process exits on its own after 10 minutes. `runon stop --demo`
//! ends them now.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const STAMPS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_PROJECT_DIR",
    "CODEX_SESSION_ID",
    "CODEX_THREAD_ID",
    "CODEX_TOOL_CALL_ID",
    "OPENCODE",
    "OPENCODE_SESSION_ID",
    "AI_AGENT",
];
pub const LIFETIME: u64 = 600;
const HOT_FOR: u64 = 120;

pub struct Demo {
    pub dir: PathBuf,
    pub sessions: [String; 3],
}

fn uid(salt: u64) -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
        ^ (u64::from(std::process::id()) << 20)
        ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let x = n.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let y = x.rotate_left(29).wrapping_mul(0x94D0_49BB_1331_11EB);
    format!(
        "{:08x}-{:04x}-4{:03x}-a{:03x}-{:012x}",
        (x >> 32) as u32,
        (x >> 16) as u16,
        x & 0xfff,
        y & 0xfff,
        y >> 16 & 0xffff_ffff_ffff
    )
}

fn exe_name(n: &str) -> String {
    format!("{n}{}", std::env::consts::EXE_SUFFIX)
}

fn cmd(bin: &Path, name: &str, args: &[&str]) -> Command {
    let mut c = Command::new(bin.join(exe_name(name)));
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for k in STAMPS {
        c.env_remove(k);
    }
    c
}

fn detach(c: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        c.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0000_0200 | 0x0000_0008); // NEW_PROCESS_GROUP | DETACHED_PROCESS
    }
}

fn line(v: serde_json::Value) -> String {
    format!("{v}\n")
}

/// Lay out the directory and start the three fake sessions.
#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    unsafe extern "system" {
        fn GetStdHandle(n: u32) -> *mut c_void;
        fn SetHandleInformation(h: *mut c_void, mask: u32, flags: u32) -> i32;
    }
    /// Windows hands every inheritable handle to every child. Our own stdout
    /// may be a pipe someone is reading to EOF; ten-minute demo actors must
    /// not keep it open.
    pub fn keep_std_handles() {
        for n in [0xFFFF_FFF6u32, 0xFFFF_FFF5, 0xFFFF_FFF4] {
            unsafe {
                let h = GetStdHandle(n);
                if !h.is_null() && h as isize != -1 {
                    SetHandleInformation(h, 1, 0);
                }
            }
        }
    }
}

pub fn start() -> Result<Demo, String> {
    #[cfg(windows)]
    win::keep_std_handles();
    let self_exe =
        std::env::current_exe().map_err(|e| format!("cannot find runon's own binary: {e}"))?;
    let tag = &uid(7)[..8];
    let dir = std::env::temp_dir().join(format!("runon-demo-{tag}"));
    let bin = dir.join("bin");
    let shop = dir.join("shop");
    std::fs::create_dir_all(&bin).map_err(|e| format!("{}: {e}", bin.display()))?;
    std::fs::create_dir_all(&shop).map_err(|e| format!("{}: {e}", shop.display()))?;
    for n in [
        "claude",
        "codex",
        "npm",
        "vite",
        "tsc",
        "sh",
        "pytest",
        "cargo",
        "github-mcp",
    ] {
        let to = bin.join(exe_name(n));
        std::fs::copy(&self_exe, &to).map_err(|e| format!("copy to {}: {e}", to.display()))?;
    }
    let sessions = [uid(1), uid(2), uid(3)];
    let (a, b, c) = (&sessions[0], &sessions[1], &sessions[2]);
    let now = crate::util::now();
    let ts = |back: u64| crate::util::iso(now.saturating_sub(back));
    let shop_s = shop.to_string_lossy().to_string();

    // Claude Code transcripts: ~/.claude/projects/<slug>/<session>.jsonl
    let proj = dir.join("claude/projects/-demo-shop");
    std::fs::create_dir_all(&proj).map_err(|e| e.to_string())?;
    let claude_log = |sid: &str, id: &str, command: &str, bg: bool| {
        line(
            serde_json::json!({"timestamp": ts(4), "type": "user", "cwd": shop_s, "sessionId": sid, "message": {"role": "user", "content": "start the dev server and keep the types checking"}}),
        ) + &line(
            serde_json::json!({"timestamp": ts(3), "type": "assistant", "cwd": shop_s, "sessionId": sid, "message": {"role": "assistant", "content": [{"type": "tool_use", "id": id, "name": "Bash", "input": {"command": command, "run_in_background": bg}}]}}),
        ) + &line(
            serde_json::json!({"timestamp": ts(3), "type": "user", "cwd": shop_s, "sessionId": sid, "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": id, "content": "Command running in background"}]}}),
        )
    };
    std::fs::write(
        proj.join(format!("{a}.jsonl")),
        claude_log(
            a,
            "toolu_01Demo8DevServer",
            "npm run dev -- --port 5173",
            true,
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(
        proj.join(format!("{c}.jsonl")),
        claude_log(c, "toolu_01Demo7TscWatch", "tsc --watch -p .", true),
    )
    .map_err(|e| e.to_string())?;

    // Codex rollout: ~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<thread>.jsonl
    let day = &ts(0)[..10];
    let sdir = dir.join("codex/sessions").join(day.replace('-', "/"));
    std::fs::create_dir_all(&sdir).map_err(|e| e.to_string())?;
    let rollout = line(
        serde_json::json!({"timestamp": ts(5), "type": "session_meta", "payload": {"id": b, "cwd": shop_s}}),
    ) + &line(
        serde_json::json!({"timestamp": ts(3), "type": "response_item", "payload": {"type": "function_call", "name": "exec_command", "call_id": "call_demo_pytest", "arguments": serde_json::json!({"cmd": "pytest -x tests/"}).to_string()}}),
    ) + &line(
        serde_json::json!({"timestamp": ts(2), "type": "response_item", "payload": {"type": "function_call_output", "call_id": "call_demo_pytest", "output": "Process running with session ID 1"}}),
    ) + &line(
        serde_json::json!({"timestamp": ts(2), "type": "response_item", "payload": {"type": "function_call", "name": "exec_command", "call_id": "call_demo_watch", "arguments": serde_json::json!({"cmd": "cargo watch -x test"}).to_string()}}),
    );
    std::fs::write(
        sdir.join(format!(
            "rollout-{}-{b}.jsonl",
            ts(5).replace(':', "-").trim_end_matches('Z')
        )),
        rollout,
    )
    .map_err(|e| e.to_string())?;

    let until = (now + LIFETIME).to_string();
    let mut kids = vec![];
    for (plan, agent, sid) in [("a", "claude", a), ("b", "codex", b), ("c", "claude", c)] {
        let mut h = cmd(&bin, agent, &[]);
        h.current_dir(&shop)
            .env("RUNON_DEMO", "1")
            .env("RUNON_DEMO_HOME", &dir)
            .env("RUNON_DEMO_ROLE", "harness")
            .env("RUNON_DEMO_PLAN", plan)
            .env("RUNON_DEMO_SID", sid)
            .env("RUNON_DEMO_BIN", &bin)
            .env("RUNON_DEMO_UNTIL", &until);
        detach(&mut h);
        kids.push(
            h.spawn()
                .map_err(|e| format!("could not start the demo {agent}: {e}"))?,
        );
    }
    // session A's harness exits on its own after ~0.6s; reap it so it is really gone
    let _ = kids.remove(0).wait();
    Ok(Demo { dir, sessions })
}

/// If this process is one of the demo actors, play the part and exit.
pub fn role() -> Option<i32> {
    let role = std::env::var("RUNON_DEMO_ROLE").ok()?;
    let until: u64 = std::env::var("RUNON_DEMO_UNTIL")
        .ok()
        .and_then(|u| u.parse().ok())
        .unwrap_or(0);
    let deadline = until.min(crate::util::now() + LIFETIME);
    match role.as_str() {
        "harness" => harness(deadline),
        "idle" => idle(deadline),
        "hot" => hot(deadline),
        "parent" => {
            // start the child given in RUNON_DEMO_CHILD ("name\x1farg\x1farg"), then idle
            if let (Ok(spec), Ok(bin)) = (
                std::env::var("RUNON_DEMO_CHILD"),
                std::env::var("RUNON_DEMO_BIN"),
            ) {
                let mut it = spec.split('\x1f');
                let name = it.next().unwrap_or("vite").to_string();
                let args: Vec<String> = it.map(String::from).collect();
                let mut c = Command::new(Path::new(&bin).join(exe_name(&name)));
                c.args(&args)
                    .env("RUNON_DEMO_ROLE", "idle")
                    .env_remove("RUNON_DEMO_CHILD")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                if c.spawn().is_ok()
                    && let Ok(home) = std::env::var("RUNON_DEMO_HOME")
                {
                    // tell the harness the child exists (process start is slow on Windows)
                    let _ = std::fs::write(Path::new(&home).join(format!(".started-{name}")), b"");
                }
            }
            idle(deadline)
        }
        _ => idle(deadline),
    };
    Some(0)
}

fn idle(deadline: u64) {
    while crate::util::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn hot(deadline: u64) {
    let hot_until = (crate::util::now() + HOT_FOR).min(deadline);
    let mut x: u64 = 1;
    while crate::util::now() < hot_until {
        let t = Instant::now();
        while t.elapsed() < Duration::from_millis(85) {
            x = std::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    idle(deadline)
}

fn harness(deadline: u64) {
    let plan = std::env::var("RUNON_DEMO_PLAN").unwrap_or_default();
    let sid = std::env::var("RUNON_DEMO_SID").unwrap_or_default();
    let bin = PathBuf::from(std::env::var("RUNON_DEMO_BIN").unwrap_or_default());
    let home = std::env::var("RUNON_DEMO_HOME").unwrap_or_default();
    let shop = Path::new(&home).join("shop");
    let tool = |name: &str, args: &[&str], role: &str| {
        let mut c = cmd(&bin, name, args);
        c.current_dir(&shop).env("RUNON_DEMO_ROLE", role);
        if plan == "b" {
            c.env("CODEX_SESSION_ID", &sid).env("CODEX_THREAD_ID", &sid);
        } else {
            c.env("CLAUDECODE", "1").env("CLAUDE_CODE_SESSION_ID", &sid);
        }
        c
    };
    match plan.as_str() {
        "a" => {
            // background `npm run dev`, which starts vite; then the session ends
            let mut n = tool("npm", &["run", "dev", "--", "--port", "5173"], "parent");
            n.env("RUNON_DEMO_CHILD", "vite\x1f--port\x1f5173");
            let _ = n.spawn();
            std::thread::sleep(Duration::from_millis(600));
        }
        "b" => {
            let _ = tool("pytest", &["-x", "tests/"], "hot")
                .env("CODEX_TOOL_CALL_ID", "call_demo_pytest")
                .spawn();
            let _ = tool("cargo", &["watch", "-x", "test"], "idle")
                .env("CODEX_TOOL_CALL_ID", "call_demo_watch")
                .spawn();
            idle(deadline);
        }
        _ => {
            // an MCP server it keeps, and a background shell it then stops the
            // way a task stop does: SIGKILL to the shell only
            let _ = tool("github-mcp", &["--stdio"], "idle")
                .env("CLAUDE_PROJECT_DIR", &shop)
                .spawn();
            let mut sh = tool("sh", &["-c", "tsc --watch -p ."], "parent");
            sh.env("RUNON_DEMO_CHILD", "tsc\x1f--watch\x1f-p\x1f.");
            if let Ok(mut s) = sh.spawn() {
                let mark = Path::new(&home).join(".started-tsc");
                let t = Instant::now();
                while !mark.exists() && t.elapsed() < Duration::from_secs(15) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                std::thread::sleep(Duration::from_millis(150));
                let _ = s.kill();
                let _ = s.wait();
            }
            idle(deadline);
        }
    }
}
