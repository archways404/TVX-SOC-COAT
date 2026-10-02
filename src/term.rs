//! Terminal summary: the route in a few lines, with the why under each hop.

use std::io::IsTerminal;

use crate::report::{route_text, short_reason};
use crate::trace::{CallTrace, Step, format_duration};

struct Paint(bool);

impl Paint {
    fn color(&self, code: &str, text: &str) -> String {
        if self.0 { format!("\x1b[{code}m{text}\x1b[0m") } else { text.to_string() }
    }
    fn bold(&self, text: &str) -> String {
        self.color("1", text)
    }
    fn dim(&self, text: &str) -> String {
        self.color("2", text)
    }
}

fn kind_color(kind: &str) -> &'static str {
    match kind {
        "refer" => "35",
        "anumber" => "34",
        "ivrscript" => "33",
        "queue" => "36",
        "agent" | "answered" => "32",
        _ => "37",
    }
}

pub fn use_color() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").map_or(true, |t| t != "dumb")
}

pub fn terminal_width() -> usize {
    std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).unwrap_or(110).clamp(60, 160)
}

pub fn render(trace: &CallTrace, color: bool, report_path: Option<&str>) -> String {
    let paint = Paint(color);
    let width = terminal_width();
    let mut lines = header(trace, &paint);
    lines.push(String::new());
    lines.push(format!("{}  {}", paint.bold("ROUTE"), one_line_route(trace, &paint)));
    lines.push(String::new());
    lines.push(paint.bold("PATH"));
    lines.extend(route_text(trace).lines().map(|l| format!("  {l}")));
    lines.push(String::new());
    lines.push(paint.bold("STEPS"));
    lines.extend(steps(trace, &paint, width));
    lines.push(String::new());
    lines.push(paint.bold("ISSUES"));
    lines.extend(issues(trace, &paint, width));
    if let Some(path) = report_path {
        lines.push(String::new());
        lines.push(format!("{}  {path}", paint.bold("REPORT")));
    }
    lines.join("\n")
}

fn header(trace: &CallTrace, paint: &Paint) -> Vec<String> {
    let bundle = &trace.bundle;
    let scrub = if bundle.scrubbed { paint.color("32", "scrubbed") } else { paint.color("1;33", "UNSCRUBBED") };
    let sessions: Vec<String> = bundle.sessions.iter()
        .map(|s| if s.sessionid == trace.root { format!("{} (root)", s.sessionid) } else { s.sessionid.clone() })
        .collect();
    let outcome = &trace.outcome;
    let mut details = Vec::new();
    if outcome.answered_at.is_some() {
        details.push(format!("after {}", format_duration(outcome.wait_seconds(trace.start))));
    }
    if let Some(talk) = outcome.talk_seconds() {
        details.push(format!("talked {}", format_duration(Some(talk))));
    }
    if !outcome.hangup_by.is_empty() {
        details.push(format!("hung up by {}", outcome.hangup_by));
    }
    let verdict = paint.color(if outcome.answered_number.is_empty() { "31" } else { "32" }, &outcome.summary);
    let details = if details.is_empty() { String::new() } else { paint.dim(&format!(" · {}", details.join(" · "))) };
    vec![
        format!("{}  {} · {} · {scrub} · sessions: {}", paint.bold("COAT"), paint.bold(&trace.root), bundle.env, sessions.join(", ")),
        format!("      {} → {}  ({})", trace.start.format("%Y-%m-%d %H:%M:%S"), trace.end.format("%H:%M:%S"),
                format_duration(Some(trace.seconds()))),
        format!("      {} calls {}", paint.bold(&trace.caller), paint.bold(&trace.dialed)),
        format!("      {verdict}{details}"),
    ]
}

fn one_line_route(trace: &CallTrace, paint: &Paint) -> String {
    let mut line = trace.caller.clone();
    for step in &trace.steps {
        let reason = step.arrival.as_ref().map_or("calls".to_string(), |a| short_reason(&a.reason));
        line.push_str(&paint.dim(&format!(" ─{reason}→ ")));
        line.push_str(&paint.color(kind_color(&step.kind), &paint.bold(&step.number)));
    }
    line
}

fn steps(trace: &CallTrace, paint: &Paint, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for step in &trace.steps {
        if let Some(arrival) = &step.arrival {
            lines.push(format!("     {}  {}", paint.dim("↓"), paint.bold(&arrival.reason)));
            for evidence in arrival.evidence.iter().take(3) {
                lines.push(paint.dim(&format!("        {}", clip(evidence, width - 10))));
            }
        }
        let name = if step.name.is_empty() { String::new() } else { format!(" · {}", step.name) };
        let title = format!("{}  {}{name}", step.number, step.label());
        let duration = format_duration(Some(step.seconds()));
        let pad = width.saturating_sub(12 + title.chars().count() + duration.len()).max(1);
        lines.push(format!("  {:>2} {}  {}{}{}", step.index, step.start.format("%H:%M:%S"),
                           paint.color(kind_color(&step.kind), &paint.bold(&title)), " ".repeat(pad), paint.dim(&duration)));
        for highlight in highlights(step) {
            lines.push(format!("              {}", clip(&highlight, width - 16)));
        }
    }
    lines
}

fn highlights(step: &Step) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["custom script", "PBX mode", "profile", "device", "rang for"] {
        if let Some(value) = step.fact(key) {
            out.push(format!("{key}: {value}"));
        }
    }
    let prompts: usize = step.activity.iter().filter(|a| a.kind == "prompt").map(|a| a.count).sum();
    if prompts > 0 {
        out.push(format!("{prompts} prompt{} played", if prompts == 1 { "" } else { "s" }));
    }
    for activity in &step.activity {
        match activity.kind.as_str() {
            "dtmf" => out.push(format!("pressed {}", activity.text)),
            "error" | "warn" => {
                let marker = if activity.kind == "error" { "✖" } else { "!" };
                let count = if activity.count > 1 { format!("  ×{}", activity.count) } else { String::new() };
                out.push(format!("{marker} {}{count}", activity.text));
            }
            "hold" => out.push(activity.text.clone()),
            _ => {}
        }
    }
    if let Some(visit) = &step.queue {
        let span = match (visit.positions.first(), visit.positions.last()) {
            (Some(first), Some(last)) => format!("position {} → {}", first.1, last.1),
            _ => "no position announcements".into(),
        };
        let wait = visit.wait_ms.map_or("?".into(), |ms| format_duration(Some(ms as f64 / 1000.0)));
        let left = if visit.leave_reason.is_empty() { "?" } else { &visit.leave_reason };
        out.push(format!("{} ({}): {span}, waited {wait}, {} rounds with no free agent, left: {left}",
                         visit.queue, visit.name, visit.no_agent_rounds));
    }
    for leg in &step.legs {
        let what = if leg.origin.is_empty() { &leg.oneliner } else { &leg.origin };
        out.push(format!("leg {}: {what} → {}", leg.session, leg.outcome()));
    }
    out
}

fn issues(trace: &CallTrace, paint: &Paint, width: usize) -> Vec<String> {
    let worth_a_look: Vec<_> = trace.issues.iter().filter(|i| !i.usually_noise).collect();
    let noise = trace.issues.len() - worth_a_look.len();
    let mut lines = Vec::new();
    for issue in worth_a_look.iter().take(8) {
        let color = if issue.level == "ERROR" { "31" } else { "33" };
        let count = if issue.count > 1 { format!(" ×{}", issue.count) } else { String::new() };
        lines.push(format!("  {:<5} {} {:<12} {}{count}", paint.color(color, &issue.level), issue.first.format("%H:%M:%S"),
                           issue.app, clip(&issue.sample, width - 40)));
        lines.push(paint.dim(&format!("        {}", issue.loc)));
    }
    if worth_a_look.is_empty() {
        lines.push(paint.color("32", "  nothing unusual"));
    }
    for failure in &trace.outcome.failures {
        lines.push(paint.color("31", &format!("  SIP   {failure}")));
    }
    lines.push(paint.dim(&format!("  + {noise} kinds of WARN that show up on most calls (see the report)")));
    lines
}

fn clip(text: &str, limit: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= limit {
        return text;
    }
    let mut clipped: String = text.chars().take(limit.saturating_sub(1).max(1)).collect();
    clipped.push('…');
    clipped
}
