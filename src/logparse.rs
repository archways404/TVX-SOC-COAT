//! Turn raw simlog rows into structured, time-sortable entries.
//!
//! Two row shapes show up in simlog:
//!
//! ```text
//! Java apps (tproxy, scriptserver, top, tlb, userdatanode, ...):
//!   Oct  2 10:22:51 proxy1 tproxy: 2026-10-02 10:22:51,835 INFO <thread> method(File.java:123) [LID:x] message
//! Plain syslog (kamailio SBCs, ...):
//!   Oct  2 10:46:20 edge1 /usr/sbin/kamailio[3680]: NOTICE: <script>: [LID:x] message
//! ```
//!
//! Anything after the first line (SIP messages, SDP, stack traces) is kept as the body.

use std::sync::LazyLock;

use chrono::{NaiveDate, NaiveDateTime};
use regex::Regex;

static SYSLOG_ROW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(?P<mon>[A-Z][a-z]{2})\s+(?P<day>\d{1,2}) (?P<hms>\d\d:\d\d:\d\d) ",
        r"(?P<host>\S+) (?P<app>[^:\s]+?)(?:\[\d+\])?: ",
        r"(?:(?P<level>[A-Z]+): )?(?P<msg>.*)$",
    ))
    .unwrap()
});

static LID_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\[LID:[^\]\s]+\]\s*").unwrap());

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

#[derive(Debug, Clone)]
pub struct Entry {
    pub seq: usize,
    pub session: String,
    pub time: NaiveDateTime,
    pub host: String,
    pub app: String,
    pub level: String,
    pub thread: String,
    pub loc: String,
    pub msg: String,
    pub body: String,
}

impl Entry {
    /// `findAnAgent` for `se.telavox.Queue.findAnAgent(Queue.java:3737)`.
    pub fn method(&self) -> &str {
        let name = self.loc.split('(').next().unwrap_or("");
        name.rsplit('.').next().unwrap_or(name)
    }

    /// `Queue.java` for `findAnAgent(Queue.java:3737)`; `?` for `call(?:0)`.
    pub fn source_file(&self) -> &str {
        let Some(open) = self.loc.find('(') else { return "" };
        let inner = self.loc[open + 1..].trim_end_matches(')');
        inner.split(':').next().unwrap_or("")
    }

    pub fn is_problem(&self) -> bool {
        self.level == "WARN" || self.level == "ERROR"
    }

    pub fn clock(&self) -> String {
        self.time.format("%H:%M:%S%.3f").to_string()
    }
}

/// Parse rows in parallel. Rows without their own timestamp inherit the previous row's.
pub fn parse_rows(rows: &[String], session: &str, year: i32, seq_start: usize) -> Vec<Entry> {
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
    let chunk = rows.len().div_ceil(workers).max(2000);
    let mut parsed: Vec<(Entry, bool)> = std::thread::scope(|scope| {
        let handles: Vec<_> = rows
            .chunks(chunk)
            .enumerate()
            .map(|(index, part)| {
                scope.spawn(move || {
                    part.iter()
                        .enumerate()
                        .map(|(offset, row)| parse_row_timed(row, session, year, seq_start + index * chunk + offset))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("parser thread panicked")).collect()
    });
    let mut previous = NaiveDate::from_ymd_opt(year, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
    for (entry, has_time) in &mut parsed {
        if *has_time {
            previous = entry.time;
        } else {
            entry.time = previous;
        }
    }
    parsed.into_iter().map(|(entry, _)| entry).collect()
}

#[cfg(test)]
pub fn parse_row(row: &str, session: &str, year: i32, seq: usize, fallback: NaiveDateTime) -> Entry {
    let (mut entry, has_time) = parse_row_timed(row, session, year, seq);
    if !has_time {
        entry.time = fallback;
    }
    entry
}

/// Returns the entry and whether the row carried its own timestamp.
fn parse_row_timed(row: &str, session: &str, year: i32, seq: usize) -> (Entry, bool) {
    let (first_line, body) = row.split_once('\n').unwrap_or((row, ""));
    if let Some(java) = split_java_row(first_line) {
        let entry = build(session, seq, body, java.time, java.host, java.app, java.level, java.thread, java.loc, java.msg);
        return (entry, true);
    }
    if let Some(syslog) = SYSLOG_ROW.captures(first_line) {
        let time = syslog_time(&syslog["mon"], &syslog["day"], &syslog["hms"], year);
        let level = syslog.name("level").map_or("INFO", |m| m.as_str());
        let entry = build(session, seq, body, time.unwrap_or_default(), &syslog["host"], &syslog["app"], level, "", "", &syslog["msg"]);
        return (entry, time.is_some());
    }
    let entry = Entry {
        seq, session: session.to_string(), time: NaiveDateTime::default(), host: "?".into(), app: "?".into(),
        level: "INFO".into(), thread: String::new(), loc: String::new(),
        msg: first_line.to_string(), body: body.to_string(),
    };
    (entry, false)
}

struct JavaRow<'a> {
    time: NaiveDateTime,
    host: &'a str,
    app: &'a str,
    level: &'a str,
    thread: &'a str,
    loc: &'a str,
    msg: &'a str,
}

/// `[Mon DD HH:MM:SS ]host app: YYYY-MM-DD HH:MM:SS,mmm LEVEL <thread> method(File.java:N) message`,
/// split by hand: a regex with a lazy thread capture was 90% of the run time.
fn split_java_row(line: &str) -> Option<JavaRow<'_>> {
    let mut rest = skip_syslog_prefix(line);
    let host = next_token(&mut rest)?;
    let app = next_token(&mut rest)?.strip_suffix(':')?;
    let date = next_token(&mut rest)?;
    let clock = next_token(&mut rest)?;
    let time = parse_iso(date, clock)?;
    let level = next_token(&mut rest)?;
    if level.is_empty() || !level.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    // The location is the first token shaped like `method(File.java:123)`; the thread
    // name (which may contain spaces) is everything before it.
    let mut offset = 0;
    loop {
        let candidate = &rest[offset..];
        let token_end = candidate.find(' ').unwrap_or(candidate.len());
        let token = &candidate[..token_end];
        if offset > 0 && is_location(token) {
            let thread = rest[..offset].trim_end();
            let msg = candidate[token_end..].strip_prefix(' ').unwrap_or(&candidate[token_end..]);
            return Some(JavaRow { time, host, app: app.rsplit('/').next().unwrap_or(app), level, thread, loc: token, msg });
        }
        if token_end == candidate.len() {
            return None;
        }
        offset += token_end + 1;
    }
}

fn skip_syslog_prefix(line: &str) -> &str {
    let bytes = line.as_bytes();
    let starts_like_month = bytes.len() > 16 && bytes[0].is_ascii_uppercase()
        && bytes[1].is_ascii_lowercase() && bytes[2].is_ascii_lowercase() && bytes[3] == b' ';
    if !starts_like_month {
        return line;
    }
    let mut rest = &line[4..];
    rest = rest.trim_start_matches(' ');
    let day_end = rest.find(' ').unwrap_or(0);
    if day_end == 0 || !rest[..day_end].bytes().all(|b| b.is_ascii_digit()) {
        return line;
    }
    let after_day = &rest[day_end + 1..];
    match after_day.get(..9) {
        Some(clock) if clock.as_bytes()[2] == b':' && clock.as_bytes()[5] == b':' && clock.ends_with(' ') => &after_day[9..],
        _ => line,
    }
}

fn next_token<'a>(rest: &mut &'a str) -> Option<&'a str> {
    let end = rest.find(' ')?;
    let token = &rest[..end];
    *rest = &rest[end + 1..];
    Some(token)
}

fn is_location(token: &str) -> bool {
    let Some(open) = token.find('(') else { return false };
    let (name, inner) = (&token[..open], &token[open + 1..]);
    !name.is_empty()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_$.<>".contains(&b))
        && inner.ends_with(')')
        && !inner[..inner.len() - 1].contains(['(', ')'])
}

/// `2026-10-02` + `10:22:51,835` without going through a format parser.
fn parse_iso(date: &str, clock: &str) -> Option<NaiveDateTime> {
    let (d, c) = (date.as_bytes(), clock.as_bytes());
    if d.len() != 10 || c.len() != 12 || d[4] != b'-' || d[7] != b'-' || c[2] != b':' || c[5] != b':' || c[8] != b',' {
        return None;
    }
    let number = |bytes: &[u8]| -> Option<u32> {
        bytes.iter().try_fold(0u32, |acc, &b| b.is_ascii_digit().then(|| acc * 10 + u32::from(b - b'0')))
    };
    NaiveDate::from_ymd_opt(number(&d[..4])? as i32, number(&d[5..7])?, number(&d[8..10])?)?
        .and_hms_milli_opt(number(&c[..2])?, number(&c[3..5])?, number(&c[6..8])?, number(&c[9..12])?)
}

#[allow(clippy::too_many_arguments)]
fn build(session: &str, seq: usize, body: &str, time: NaiveDateTime, host: &str, app: &str, level: &str,
         thread: &str, loc: &str, raw_msg: &str) -> Entry {
    let msg = if raw_msg.contains("[LID:") {
        LID_TAG.replace_all(raw_msg, " ").trim().to_string()
    } else {
        raw_msg.trim().to_string()
    };
    Entry {
        seq,
        session: session.to_string(),
        time,
        host: host.to_string(),
        app: app.rsplit('/').next().unwrap_or(app).to_string(),
        level: normalize_level(level).to_string(),
        thread: thread.to_string(),
        loc: loc.to_string(),
        msg,
        body: body.trim_end().to_string(),
    }
}

fn normalize_level(level: &str) -> &str {
    match level {
        "WARNING" => "WARN",
        "NOTICE" => "INFO",
        "ERR" | "CRIT" => "ERROR",
        other => other,
    }
}

fn syslog_time(month: &str, day: &str, hms: &str, year: i32) -> Option<NaiveDateTime> {
    let month = MONTHS.iter().position(|m| *m == month)? as u32 + 1;
    let date = NaiveDate::from_ymd_opt(year, month, day.parse().ok()?)?;
    let mut parts = hms.split(':').map(|p| p.parse::<u32>().ok());
    date.and_hms_opt(parts.next()??, parts.next()??, parts.next()??)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jan1() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap()
    }

    #[test]
    fn java_row_with_spaces_in_thread_and_sip_body() {
        let row = "Oct  2 10:46:20 proxy1 tproxy: 2026-10-02 10:46:20,570 INFO New I/O worker #4 \
                   logFromCorrelation(ApplicationSession.java:425) assoc:X [LID:abc] hello\nINVITE sip:x SIP/2.0";
        let entry = parse_row(row, "abc", 2026, 0, jan1());
        assert_eq!((entry.host.as_str(), entry.app.as_str(), entry.level.as_str()), ("proxy1", "tproxy", "INFO"));
        assert_eq!(entry.thread, "New I/O worker #4");
        assert_eq!(entry.loc, "logFromCorrelation(ApplicationSession.java:425)");
        assert_eq!(entry.method(), "logFromCorrelation");
        assert_eq!(entry.source_file(), "ApplicationSession.java");
        assert_eq!(entry.msg, "assoc:X hello");
        assert_eq!(entry.body, "INVITE sip:x SIP/2.0");
        assert_eq!(entry.clock(), "10:46:20.570");
    }

    #[test]
    fn script_location_without_file() {
        let row = "Oct  2 10:23:51 proxy1 scriptserver: 2026-10-02 10:23:51,609 WARN AGIScriptHandler-1 call(?:0) [LID:x] account=1, rm=false";
        let entry = parse_row(row, "x", 2026, 0, jan1());
        assert_eq!((entry.level.as_str(), entry.loc.as_str(), entry.msg.as_str()), ("WARN", "call(?:0)", "account=1, rm=false"));
        assert_eq!(entry.source_file(), "?");
    }

    #[test]
    fn kamailio_syslog_row() {
        let row = "Oct  2 10:46:20 edge1 /usr/sbin/kamailio[3680]: NOTICE: <script>: [LID:x] Call to user abc";
        let entry = parse_row(row, "x", 2026, 0, jan1());
        assert_eq!((entry.host.as_str(), entry.app.as_str(), entry.level.as_str()), ("edge1", "kamailio", "INFO"));
        assert_eq!(entry.clock(), "10:46:20.000");
    }
}
