//! Small helpers: time, sizes, colour, ISO-8601 parsing. No dependencies on purpose.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "2h 14m", "3m 05s", "41s", "6d 2h".
pub fn age(secs: u64) -> String {
    let (d, h, m, s) = (secs / 86400, secs / 3600 % 24, secs / 60 % 60, secs % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

pub fn bytes(n: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mb = n as f64 / MB;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else if mb >= 10.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{mb:.1} MB")
    }
}

/// Parse `2026-10-23T02:56:01.123Z` / `+08:00` offsets into unix seconds.
pub fn parse_iso(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || (b[10] != b'T' && b[10] != b' ') {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, se) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    // days from civil (Howard Hinnant)
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let mut t = days * 86400 + h * 3600 + mi * 60 + se;
    // offset
    let rest = &s[19..];
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if let Some(sign @ ('+' | '-')) = rest.chars().next() {
        let oh = rest.get(1..3)?.parse::<i64>().ok()?;
        let om = rest
            .get(4..6)
            .and_then(|x| x.parse::<i64>().ok())
            .unwrap_or(0);
        let off = oh * 3600 + om * 60;
        t -= if sign == '+' { off } else { -off };
    }
    u64::try_from(t).ok()
}

pub fn iso(t: u64) -> String {
    let days = (t / 86400) as i64;
    let secs = t % 86400;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

pub fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Shorten a path with ~ for the home directory.
pub fn tilde(p: &str) -> String {
    if let Some(h) = home() {
        let h = h.to_string_lossy().to_string();
        if !h.is_empty() && p.starts_with(&h) {
            return format!("~{}", &p[h.len()..]);
        }
    }
    p.to_string()
}

pub fn home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(Into::into)
}

/// Cut a string to `n` display chars with an ellipsis.
pub fn clip(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[derive(Clone, Copy)]
pub struct Paint {
    pub on: bool,
}

impl Paint {
    pub fn detect(force_off: bool) -> Paint {
        use std::io::IsTerminal;
        let on = !force_off
            && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
            && std::io::stdout().is_terminal()
            && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(true);
        Paint { on }
    }
    fn wrap(&self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    /// vermilion, reverse block: the OUTLIVED badge
    pub fn outlived(&self, s: &str) -> String {
        self.wrap("1;97;48;2;226;73;47", s)
    }
    /// ochre badge: RETURNED
    pub fn returned(&self, s: &str) -> String {
        self.wrap("1;30;48;2;217;142;4", s)
    }
    /// bottle-green badge: LIVE
    pub fn live(&self, s: &str) -> String {
        self.wrap("1;97;48;2;30;51;38", s)
    }
    pub fn red(&self, s: &str) -> String {
        self.wrap("38;2;226;73;47", s)
    }
    pub fn ochre(&self, s: &str) -> String {
        self.wrap("38;2;217;142;4", s)
    }
    pub fn green(&self, s: &str) -> String {
        self.wrap("38;2;111;170;120", s)
    }
    pub fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iso_roundtrip() {
        let t = parse_iso("2026-10-23T02:56:01.123Z").unwrap();
        assert_eq!(iso(t), "2026-10-23T02:56:01Z");
        assert_eq!(parse_iso("2026-10-23T10:56:01+08:00"), Some(t));
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("nope"), None);
    }
    #[test]
    fn ages() {
        assert_eq!(age(41), "41s");
        assert_eq!(age(185), "3m 05s");
        assert_eq!(age(7_980), "2h 13m");
        assert_eq!(age(3 * 86400 + 7200), "3d 2h");
    }
    #[test]
    fn clipping() {
        assert_eq!(clip("abcdef", 4), "abc…");
        assert_eq!(clip("abc", 4), "abc");
    }
}
