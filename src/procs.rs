//! A point-in-time snapshot of the process table, with only the bits runon needs.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Environment variables that coding-agent harnesses stamp on the commands they run.
pub const KEYS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_PROJECT_DIR",
    "CODEX_SESSION_ID",
    "CODEX_THREAD_ID",
    "CODEX_TOOL_CALL_ID",
    "OPENCODE",
    "OPENCODE_SESSION_ID",
    "OPENCODE_PID",
    "AI_AGENT",
    "RUNON_DEMO",
    "RUNON_DEMO_HOME",
];

#[derive(Clone, Debug, Default)]
pub struct Proc {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub cmd: Vec<String>,
    pub cwd: Option<String>,
    /// unix seconds
    pub start: u64,
    /// percent of one core
    pub cpu: f32,
    pub rss: u64,
    pub env: BTreeMap<String, String>,
    pub zombie: bool,
}

impl Proc {
    pub fn env(&self, k: &str) -> Option<&str> {
        self.env
            .get(k)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }
    /// argv with argv[0] shortened to its file name.
    pub fn command(&self) -> String {
        let mut parts = self.cmd.clone();
        if parts.is_empty() {
            return format!("[{}]", self.name);
        }
        let b = base(&parts[0]);
        let b = match b.len().checked_sub(4) {
            Some(i)
                if cfg!(windows)
                    && b.is_char_boundary(i)
                    && b[i..].eq_ignore_ascii_case(".exe") =>
            {
                &b[..i]
            }
            _ => b,
        };
        parts[0] = b.to_string();
        parts.join(" ")
    }
    pub fn joined(&self) -> String {
        self.cmd.join(" ")
    }
}

pub fn base(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

#[derive(Default)]
pub struct Snapshot {
    pub procs: HashMap<u32, Proc>,
    pub now: u64,
    pub self_pid: u32,
    pub took: Duration,
}

impl Snapshot {
    pub fn get(&self, pid: u32) -> Option<&Proc> {
        self.procs.get(&pid)
    }

    /// Parent, but only if it really is the parent: a parent that started after
    /// its child is a recycled PID (Windows never reparents orphans).
    pub fn parent(&self, p: &Proc) -> Option<&Proc> {
        let pp = self.procs.get(&p.ppid?)?;
        if pp.pid == p.pid || pp.start > p.start + 1 {
            return None;
        }
        Some(pp)
    }

    pub fn children(&self) -> HashMap<u32, Vec<u32>> {
        let mut m: HashMap<u32, Vec<u32>> = HashMap::new();
        for p in self.procs.values() {
            if let Some(pp) = self.parent(p) {
                m.entry(pp.pid).or_default().push(p.pid);
            }
        }
        for v in m.values_mut() {
            v.sort_unstable();
        }
        m
    }

    /// `pid` and every live ancestor above it.
    pub fn lineage(&self, pid: u32) -> Vec<u32> {
        let mut out = vec![];
        let mut cur = self.procs.get(&pid);
        while let Some(p) = cur {
            if out.contains(&p.pid) || out.len() > 512 {
                break;
            }
            out.push(p.pid);
            cur = self.parent(p);
        }
        out
    }

    pub fn take() -> Snapshot {
        let t0 = Instant::now();
        let mut sys = System::new();
        let light = ProcessRefreshKind::nothing().with_cpu().without_tasks();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, light);
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(300)));
        let full = ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_cmd(UpdateKind::Always)
            .with_environ(UpdateKind::Always)
            .with_cwd(UpdateKind::Always)
            .without_tasks();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, full);
        let mut procs = HashMap::new();
        for (pid, p) in sys.processes() {
            if p.thread_kind().is_some() {
                continue;
            }
            // every console program on Windows gets a conhost.exe child from the
            // system; it is not something the agent started and it goes with its owner
            if cfg!(windows) && p.name().eq_ignore_ascii_case("conhost.exe") {
                continue;
            }
            let mut env = BTreeMap::new();
            for kv in p.environ() {
                let kv = kv.to_string_lossy();
                if let Some((k, v)) = kv.split_once('=') {
                    let k = if cfg!(windows) {
                        k.to_ascii_uppercase()
                    } else {
                        k.to_string()
                    };
                    if KEYS.contains(&k.as_str()) {
                        env.insert(k, v.to_string());
                    }
                }
            }
            let pid = pid.as_u32();
            procs.insert(
                pid,
                Proc {
                    pid,
                    ppid: p.parent().map(|x| x.as_u32()),
                    name: p.name().to_string_lossy().to_string(),
                    cmd: p
                        .cmd()
                        .iter()
                        .map(|a| a.to_string_lossy().to_string())
                        .collect(),
                    cwd: p.cwd().map(|c| c.to_string_lossy().to_string()),
                    start: p.start_time(),
                    cpu: p.cpu_usage(),
                    rss: p.memory(),
                    env,
                    zombie: matches!(p.status(), sysinfo::ProcessStatus::Zombie),
                },
            );
        }
        Snapshot {
            procs,
            now: crate::util::now(),
            self_pid: std::process::id(),
            took: t0.elapsed(),
        }
    }
}

/// Is (pid, start) still the same process? Used right before every signal.
pub fn alive(pid: u32, start: u64) -> bool {
    let mut sys = System::new();
    let p = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[p]),
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );
    match sys.process(p) {
        Some(pr) => {
            pr.start_time().abs_diff(start) <= 1
                && !matches!(
                    pr.status(),
                    sysinfo::ProcessStatus::Zombie | sysinfo::ProcessStatus::Dead
                )
        }
        None => false,
    }
}

pub enum Sig {
    Term,
    Kill,
}

/// Send a signal. Returns Err(reason) if the OS refused.
pub fn signal(pid: u32, sig: Sig) -> Result<(), String> {
    #[cfg(unix)]
    {
        let s = match sig {
            Sig::Term => libc::SIGTERM,
            Sig::Kill => libc::SIGKILL,
        };
        // SAFETY: plain kill(2) on a pid we just re-verified.
        let r = unsafe { libc::kill(pid as libc::pid_t, s) };
        if r == 0 {
            Ok(())
        } else {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(libc::ESRCH) {
                Ok(())
            } else {
                Err(e.to_string())
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = sig;
        let mut sys = System::new();
        let p = sysinfo::Pid::from_u32(pid);
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[p]),
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );
        match sys.process(p) {
            None => Ok(()),
            Some(pr) if pr.kill() => Ok(()),
            Some(_) => Err("TerminateProcess was refused".into()),
        }
    }
}
