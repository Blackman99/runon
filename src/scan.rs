//! Classify agent-started processes: which harness stamped them, and is that
//! harness still holding on to them.

use crate::procs::{Proc, Snapshot, base};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Agent {
    Claude,
    Codex,
    OpenCode,
}

impl Agent {
    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::OpenCode => "opencode",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub agent: Agent,
    pub session: Option<String>,
    pub call: Option<String>,
}

/// Read the env stamps a harness put on a process, outermost-agnostic.
/// Claude Code: CLAUDECODE=1 + CLAUDE_CODE_SESSION_ID. Codex: CODEX_THREAD_ID /
/// CODEX_SESSION_ID + CODEX_TOOL_CALL_ID. OpenCode: OPENCODE=1 + OPENCODE_SESSION_ID or OPENCODE_PID.
pub fn stamps(p: &Proc) -> Vec<Stamp> {
    let mut v = vec![];
    if p.env("CLAUDE_CODE_SESSION_ID").is_some() || p.env("CLAUDECODE") == Some("1") {
        v.push(Stamp {
            agent: Agent::Claude,
            session: p.env("CLAUDE_CODE_SESSION_ID").map(String::from),
            call: None,
        });
    }
    if let Some(s) = p.env("CODEX_THREAD_ID").or(p.env("CODEX_SESSION_ID")) {
        v.push(Stamp {
            agent: Agent::Codex,
            session: Some(s.to_string()),
            call: p.env("CODEX_TOOL_CALL_ID").map(String::from),
        });
    }
    if p.env("OPENCODE_SESSION_ID").is_some() || p.env("OPENCODE") == Some("1") {
        v.push(Stamp {
            agent: Agent::OpenCode,
            // OpenCode 1.19 sets OPENCODE_PID (the harness pid), not a session id
            session: p
                .env("OPENCODE_SESSION_ID")
                .map(String::from)
                .or_else(|| p.env("OPENCODE_PID").map(|x| format!("pid:{x}"))),
            call: None,
        });
    }
    v
}

/// Is this process itself a coding-agent harness?
pub fn harness(p: &Proc) -> Option<Agent> {
    let strip = |s: &str| {
        let s = base(s).to_ascii_lowercase();
        s.strip_suffix(".exe").map(String::from).unwrap_or(s)
    };
    let by_name = |n: &str| match n {
        "claude" => Some(Agent::Claude),
        "codex" => Some(Agent::Codex),
        "opencode" => Some(Agent::OpenCode),
        _ => None,
    };
    let name = strip(&p.name);
    if let Some(a) = by_name(&name) {
        return Some(a);
    }
    if let Some(a) = p.cmd.first().and_then(|c| by_name(&strip(c))) {
        return Some(a);
    }
    if matches!(name.as_str(), "node" | "bun" | "deno" | "node-main") {
        let j = p.joined().to_ascii_lowercase().replace('\\', "/");
        if j.contains("@anthropic-ai/claude-code") || j.contains("/claude-code/cli") {
            return Some(Agent::Claude);
        }
        if j.contains("@openai/codex") || j.contains("/bin/codex.js") {
            return Some(Agent::Codex);
        }
        if j.contains("opencode-ai") || j.contains("/opencode/bin") {
            return Some(Agent::OpenCode);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    /// no live harness above it any more
    Outlived,
    /// harness alive, but the tool call that started it already returned (Codex)
    Returned,
    /// still attached to a running harness
    Live,
}

impl Verdict {
    pub fn word(self) -> &'static str {
        match self {
            Verdict::Outlived => "OUTLIVED",
            Verdict::Returned => "RETURNED",
            Verdict::Live => "LIVE",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Origin {
    pub tool: String,
    pub call_id: Option<String>,
    pub command: String,
    pub at: Option<u64>,
    pub returned_at: Option<u64>,
    pub background: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SessionInfo {
    pub transcript: Option<String>,
    pub project: Option<String>,
    pub last_write: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub verdict: Verdict,
    pub stamp: Stamp,
    pub root: u32,
    /// root first, then descendants (depth first)
    pub pids: Vec<u32>,
    pub cpu: f32,
    pub rss: u64,
    pub start: u64,
    pub tags: Vec<&'static str>,
    /// the harness this tree is attached to (LIVE / RETURNED)
    pub harness_pid: Option<u32>,
    /// a harness still running this session, if we can see one
    pub session_harness: Option<u32>,
    pub origin: Option<Origin>,
    pub session: SessionInfo,
    pub demo: bool,
}

pub struct Report {
    pub snap: Snapshot,
    pub groups: Vec<Group>,
    pub stamped: usize,
    pub harnesses: Vec<(u32, Agent)>,
}

/// Nearest live harness above `p`, walking only through verified parents.
pub fn harness_above(s: &Snapshot, p: &Proc) -> Option<(u32, Agent)> {
    let mut cur = s.parent(p);
    let mut n = 0;
    while let Some(pp) = cur {
        if let Some(a) = harness(pp) {
            return Some((pp.pid, a));
        }
        n += 1;
        if n > 512 {
            break;
        }
        cur = s.parent(pp);
    }
    None
}

pub fn classify(s: Snapshot) -> Report {
    let mine: HashSet<u32> = s.lineage(s.self_pid).into_iter().collect();
    let harnesses: Vec<(u32, Agent)> = {
        let mut v: Vec<_> = s
            .procs
            .values()
            .filter_map(|p| harness(p).map(|a| (p.pid, a)))
            .collect();
        v.sort_unstable();
        v
    };
    let hset: HashSet<u32> = harnesses.iter().map(|h| h.0).collect();

    // candidates: stamped, not a harness, not runon or its ancestors, not a zombie
    let mut cand: HashMap<u32, (Stamp, Option<(u32, Agent)>)> = HashMap::new();
    for p in s.procs.values() {
        if mine.contains(&p.pid) || hset.contains(&p.pid) || p.zombie {
            continue;
        }
        let st = stamps(p);
        if st.is_empty() {
            continue;
        }
        let above = harness_above(&s, p);
        let primary = above
            .and_then(|(_, a)| st.iter().find(|x| x.agent == a).cloned())
            .unwrap_or_else(|| st[0].clone());
        cand.insert(p.pid, (primary, above));
    }
    let stamped = cand.len();

    let kids = s.children();
    let is_root = |pid: u32| -> bool {
        let p = &s.procs[&pid];
        let (st, above) = &cand[&pid];
        match s.parent(p).and_then(|pp| cand.get(&pp.pid)) {
            Some((pst, pabove)) => {
                pst.session != st.session || pabove.map(|x| x.0) != above.map(|x| x.0)
            }
            None => true,
        }
    };
    let roots: HashSet<u32> = cand.keys().copied().filter(|&p| is_root(p)).collect();

    let mut groups = vec![];
    for &r in &roots {
        let (stamp, above) = cand[&r].clone();
        let mut pids = vec![];
        let mut stack = vec![r];
        while let Some(x) = stack.pop() {
            if pids.contains(&x)
                || (x != r && roots.contains(&x))
                || hset.contains(&x)
                || mine.contains(&x)
            {
                continue;
            }
            pids.push(x);
            if let Some(ch) = kids.get(&x) {
                for c in ch.iter().rev() {
                    stack.push(*c);
                }
            }
        }
        let ps: Vec<&Proc> = pids.iter().filter_map(|p| s.get(*p)).collect();
        let cpu: f32 = ps.iter().map(|p| p.cpu).sum();
        let rss: u64 = ps.iter().map(|p| p.rss).sum();
        let mut tags = vec![];
        if cpu >= 50.0 {
            tags.push("HOT");
        }
        if rss >= 500 * 1024 * 1024 {
            tags.push("BIG");
        }
        if ps.iter().any(|p| looks_mcp(p)) {
            tags.push("MCP");
        }
        let demo = s.procs[&r].env("RUNON_DEMO").is_some();
        if demo {
            tags.push("DEMO");
        }
        groups.push(Group {
            verdict: if above.is_some() {
                Verdict::Live
            } else {
                Verdict::Outlived
            },
            stamp,
            root: r,
            pids,
            cpu,
            rss,
            start: s.procs[&r].start,
            tags,
            harness_pid: above.map(|x| x.0),
            session_harness: None,
            origin: None,
            session: SessionInfo::default(),
            demo,
        });
    }

    // which sessions still have a harness we can see?
    let mut alive: HashMap<String, u32> = HashMap::new();
    for g in &groups {
        if let (Some(h), Some(sess)) = (g.harness_pid, &g.stamp.session) {
            alive.entry(sess.clone()).or_insert(h);
        }
    }
    for (hp, a) in &harnesses {
        if *a == Agent::OpenCode {
            alive.entry(format!("pid:{hp}")).or_insert(*hp);
        }
        // `claude --resume <id>`, `codex resume <id>`, `opencode -s <id>`
        for arg in &s.procs[hp].cmd {
            if arg.len() >= 8 && arg.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                alive.entry(arg.clone()).or_insert(*hp);
            }
        }
    }
    for g in &mut groups {
        if let Some(sess) = &g.stamp.session {
            g.session_harness = g.harness_pid.or_else(|| {
                // a harness that started after the process cannot be its session (PID reuse)
                alive
                    .get(sess)
                    .copied()
                    .filter(|h| s.procs.get(h).is_some_and(|hp| hp.start <= g.start))
            });
        }
    }
    groups.sort_by(|a, b| {
        a.verdict
            .cmp(&b.verdict)
            .then(a.start.cmp(&b.start))
            .then(a.root.cmp(&b.root))
    });
    Report {
        snap: s,
        groups,
        stamped,
        harnesses,
    }
}

pub fn looks_mcp(p: &Proc) -> bool {
    p.cmd.iter().take(4).any(|a| {
        let a = base(a).to_ascii_lowercase();
        a.contains("mcp")
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::procs::Proc;
    use std::collections::BTreeMap;

    pub fn p(pid: u32, ppid: u32, name: &str, start: u64, env: &[(&str, &str)]) -> Proc {
        Proc {
            pid,
            ppid: Some(ppid),
            name: name.into(),
            cmd: vec![format!("/usr/bin/{name}")],
            start,
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>(),
            ..Default::default()
        }
    }

    pub fn snap(ps: Vec<Proc>) -> Snapshot {
        Snapshot {
            procs: ps.into_iter().map(|p| (p.pid, p)).collect(),
            now: 10_000,
            self_pid: 999_999,
            took: Default::default(),
        }
    }

    const CL: &[(&str, &str)] = &[
        ("CLAUDECODE", "1"),
        ("CLAUDE_CODE_SESSION_ID", "3f2a91c4-aaaa"),
    ];

    #[test]
    fn attached_is_live_and_orphan_is_outlived() {
        let r = classify(snap(vec![
            p(1, 0, "init", 0, &[]),
            p(10, 1, "zsh", 100, &[]),
            p(20, 10, "claude", 200, &[]),
            p(30, 20, "bash", 300, CL),
            p(31, 30, "node", 301, CL),
            // shell was killed, child reparented to init
            p(40, 1, "vite", 302, CL),
            p(41, 40, "esbuild", 303, CL),
        ]));
        assert_eq!(r.groups.len(), 2);
        let o = &r.groups[0];
        assert_eq!(
            (o.verdict, o.root, o.pids.clone()),
            (Verdict::Outlived, 40, vec![40, 41])
        );
        assert_eq!(
            o.session_harness,
            Some(20),
            "session still alive via pid 20"
        );
        let l = &r.groups[1];
        assert_eq!(
            (l.verdict, l.root, l.pids.clone()),
            (Verdict::Live, 30, vec![30, 31])
        );
        assert_eq!(l.harness_pid, Some(20));
    }

    #[test]
    fn opencode_pid_stamp_links_session() {
        let oc: &[(&str, &str)] = &[("OPENCODE", "1"), ("OPENCODE_PID", "20")];
        let r = classify(snap(vec![
            p(20, 1, "opencode", 200, &[]),
            p(40, 1, "sleep", 302, oc),
        ]));
        assert_eq!(r.groups[0].verdict, Verdict::Outlived);
        assert_eq!(r.groups[0].stamp.session.as_deref(), Some("pid:20"));
        assert_eq!(r.groups[0].session_harness, Some(20));
        // recycled: an opencode that started after the orphan is not its session
        let r = classify(snap(vec![
            p(20, 1, "opencode", 900, &[]),
            p(40, 1, "sleep", 302, oc),
        ]));
        assert_eq!(r.groups[0].session_harness, None);
    }

    #[test]
    fn recycled_parent_pid_does_not_count_as_harness() {
        // pid 20 is a *new* claude that started after the orphan: not its parent
        let r = classify(snap(vec![
            p(20, 1, "claude", 900, &[]),
            p(40, 20, "sleep", 302, CL),
        ]));
        assert_eq!(r.groups[0].verdict, Verdict::Outlived);
        assert_eq!(r.groups[0].harness_pid, None);
    }

    #[test]
    fn harness_runon_and_unstamped_are_never_candidates() {
        let mut s = snap(vec![
            p(20, 1, "claude", 200, CL),
            p(21, 1, "python3", 200, &[]),
            p(22, 20, "bash", 210, CL),
            p(23, 22, "runon", 220, CL),
        ]);
        s.self_pid = 23;
        let r = classify(s);
        assert!(
            r.groups.is_empty(),
            "{:?}",
            r.groups.iter().map(|g| g.root).collect::<Vec<_>>()
        );
    }

    #[test]
    fn node_wrapped_harness_and_windows_exe_are_detected() {
        let mut n = p(5, 1, "node", 1, &[]);
        n.cmd = vec![
            "node".into(),
            "/usr/lib/node_modules/@openai/codex/bin/codex.js".into(),
        ];
        assert_eq!(harness(&n), Some(Agent::Codex));
        assert_eq!(harness(&p(6, 1, "claude.exe", 1, &[])), Some(Agent::Claude));
        assert_eq!(harness(&p(7, 1, "claude-helper", 1, &[])), None);
    }

    #[test]
    fn codex_stamp_carries_tool_call() {
        let x = p(
            1,
            0,
            "sh",
            0,
            &[
                ("CODEX_THREAD_ID", "019a"),
                ("CODEX_TOOL_CALL_ID", "call_9"),
            ],
        );
        let st = stamps(&x);
        assert_eq!(st[0].agent, Agent::Codex);
        assert_eq!(st[0].call.as_deref(), Some("call_9"));
    }

    #[test]
    fn nested_agents_pick_the_harness_actually_above() {
        let both = &[
            ("CODEX_THREAD_ID", "019a"),
            ("CLAUDECODE", "1"),
            ("CLAUDE_CODE_SESSION_ID", "c1"),
        ];
        let r = classify(snap(vec![
            p(20, 1, "codex", 100, &[]),
            p(30, 20, "make", 200, both),
        ]));
        assert_eq!(r.groups[0].stamp.agent, Agent::Codex);
    }

    #[test]
    fn tags() {
        let mut hot = p(40, 1, "pytest", 300, CL);
        hot.cpu = 97.0;
        hot.rss = 600 * 1024 * 1024;
        let mut m = p(50, 1, "node", 300, CL);
        m.cmd = vec!["github-mcp".into()];
        let r = classify(snap(vec![hot, m]));
        assert_eq!(r.groups[0].tags, vec!["HOT", "BIG"]);
        assert_eq!(r.groups[1].tags, vec!["MCP"]);
    }
}
