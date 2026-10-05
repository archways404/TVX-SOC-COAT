//! Self-contained HTML report. No external requests, so it's safe to open offline.

use std::fmt::Write;
use std::sync::LazyLock;

use chrono::NaiveDateTime;
use regex::Regex;
use serde_json::{Value, json};

use crate::trace::{Activity, CallTrace, QueueVisit, Step, format_duration};
pub use crate::ui::esc;
use crate::ui::{NavGroup, NavItem, Page, SubItem, icon};

const REPORT_JS: &str = include_str!("assets/report.js");

static SHORT_ANSWERED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^answered by queue agent after (.+)$").unwrap());

pub struct Options {
    /// Embed every log row, chatty ones too.
    pub full_log: bool,
    /// Rendered by the running app: adds search, recent traces, updates, Quit and a refresh link.
    pub served: Option<Served>,
}

pub struct Served {
    pub refresh_url: String,
}

fn hms(time: NaiveDateTime) -> String {
    time.format("%H:%M:%S").to_string()
}

fn embed_json(value: &Value) -> String {
    value.to_string().replace("</", "<\\/")
}

/// "answered by queue agent after 21m 47s" is too long for the route line.
pub fn short_reason(reason: &str) -> String {
    if let Some(c) = SHORT_ANSWERED.captures(reason) {
        return format!("agent answered · {}", &c[1]);
    }
    reason.replace("IVR script redirect", "script redirect")
}

pub fn render(trace: &CallTrace, options: &Options) -> String {
    let mut body = String::new();
    body.push_str(&hero(trace));
    body.push_str(&route_line(trace));
    body.push_str(&time_bar(trace));
    body.push_str(&section("Steps", "steps", &trace.steps.iter().map(|s| step_card(trace, s)).collect::<String>()));
    body.push_str(&section("Issues", "issues", &issues(trace)));
    body.push_str(&section("Variables", "variables", &variables(trace)));
    body.push_str(&section("SIP ladder", "sip", &sip_ladder(trace)));
    body.push_str(&section("Call summaries", "summaries", &call_summaries(trace)));
    body.push_str(&section("Log", "log", &log_explorer(trace, options.full_log)));
    crate::ui::render(&Page {
        title: format!("COAT {}", trace.root),
        crumbs: vec![format!("{} → {}", trace.caller, trace.dialed)],
        header_right: header_badges(trace, options.served.as_ref()),
        nav: navigation(trace),
        body,
        script: REPORT_JS.to_string(),
        served: options.served.is_some(),
        root: trace.root.clone(),
    })
}

/// The sidebar for a report: every section, and every step under "Steps".
fn navigation(trace: &CallTrace) -> Vec<NavGroup> {
    let item = |href: &str, label: &str, icon: &'static str| NavItem {
        href: format!("#{href}"), label: label.into(), icon, spy: Some(href.into()), badge: None, children: vec![],
    };
    let worth_a_look = trace.issues.iter().filter(|i| !i.usually_noise).count();
    let steps = NavItem {
        children: trace.steps.iter().map(|s| SubItem {
            href: format!("#step-{}", s.index),
            label: format!("{} · {}", s.number, if s.name.is_empty() { s.label() } else { s.name.clone() }),
            spy: format!("step-{}", s.index),
            kind: s.kind.clone(),
        }).collect(),
        badge: Some((trace.steps.len().to_string(), false)),
        ..item("steps", "Steps", "steps")
    };
    let issues = NavItem {
        badge: (worth_a_look > 0).then(|| (worth_a_look.to_string(), true)),
        ..item("issues", "Issues", "alert")
    };
    let variables = NavItem { badge: Some((trace.variables.len().to_string(), false)), ..item("variables", "Variables", "braces") };
    vec![NavGroup {
        label: "This call".into(),
        items: vec![
            item("overview", "Overview", "overview"),
            item("route", "Route", "route"),
            item("time", "Where the time went", "clock"),
            steps,
            issues,
            variables,
            item("sip", "SIP ladder", "ladder"),
            item("summaries", "Call summaries", "chart"),
            item("log", "Log", "log"),
        ],
    }]
}

fn header_badges(trace: &CallTrace, served: Option<&Served>) -> String {
    let scrub = if trace.bundle.scrubbed {
        "<span class='badge ok'>scrubbed</span>"
    } else {
        "<span class='badge warn' title='Contains personal data. Do not paste or share.'>UNSCRUBBED · personal data</span>"
    };
    let refresh = served.map_or(String::new(), |s| format!(
        "<a class='icon-btn' href='{}' title='Fetch this session again from simlog'>{}</a>", esc(&s.refresh_url), icon("refresh")));
    format!("<code class='hide-sm'>{}</code><span class='badge hide-sm'>{}</span>{scrub}{refresh}",
            esc(&trace.root), esc(&trace.bundle.env))
}

// ---- top of the page -------------------------------------------------------------------------

fn hero(trace: &CallTrace) -> String {
    let outcome = &trace.outcome;
    let mut pills = vec![
        format!("<span class='pill'>{} → {}</span>", trace.start.format("%Y-%m-%d %H:%M:%S"), hms(trace.end)),
        format!("<span class='pill'>total {}</span>", format_duration(Some(trace.seconds()))),
    ];
    if outcome.answered_at.is_some() {
        pills.push(format!("<span class='pill'>waited {}</span>", format_duration(outcome.wait_seconds(trace.start))));
    }
    if let Some(talk) = outcome.talk_seconds() {
        pills.push(format!("<span class='pill'>talked {}</span>", format_duration(Some(talk))));
    }
    if !outcome.hangup_by.is_empty() {
        pills.push(format!("<span class='pill' title='{}'>hung up by {}</span>", esc(&outcome.hangup_detail), esc(&outcome.hangup_by)));
    }
    let state = if outcome.answered_number.is_empty() { "bad" } else { "ok" };
    let sessions: String = trace.bundle.sessions.iter()
        .map(|s| {
            let root = if s.sessionid == trace.root { " <small>root</small>" } else { "" };
            format!("<code title='{}'>{}</code>{root} ", esc(&s.oneliner()), esc(&s.sessionid))
        })
        .collect();
    let oneliner = trace.bundle.session(&trace.root).map(|s| s.oneliner()).unwrap_or_default();
    let simlog_says = if oneliner.is_empty() { String::new() } else { format!(" · simlog says: “{}”", esc(&oneliner)) };
    format!(
        "<section class='hero' id='overview'><h1><span>{}</span><i>calls</i><span>{}</span></h1>\
         <p class='verdict {state}'>{}</p><div class='pills'>{}</div>\
         <p class='muted small'>Sessions: {sessions}{simlog_says}</p></section>",
        esc(&trace.caller), esc(&trace.dialed), esc(&outcome.summary), pills.join(""))
}

/// The whole route on one line: stations joined by hops labelled with the reason.
fn route_line(trace: &CallTrace) -> String {
    let mut line = format!(
        "<div class='stop caller' title='Caller'><span class='dot'></span><b>{}</b><small class='kind'>Caller</small></div>",
        esc(&trace.caller));
    for step in &trace.steps {
        let reason = step.arrival.as_ref().map_or("calls".to_string(), |a| a.reason.clone());
        let _ = write!(line, "<div class='hop' title='{}'><span>{}</span></div>", esc(&reason), esc(&short_reason(&reason)));
        let name = if step.name.is_empty() { String::new() } else { format!("<small class='nm'>{}</small>", esc(&step.name)) };
        let _ = write!(line,
            "<a class='stop k-{kind}' href='#step-{index}' title='{title}'><span class='dot'></span><b>{number}</b>\
             <small class='kind'>{label}</small>{name}<small class='dur'>{dur}</small></a>",
            kind = esc(&step.kind), index = step.index, number = esc(&step.number), label = esc(&step.label()),
            title = esc(&format!("{} · {}{}", step.number, step.label(), if step.name.is_empty() { String::new() } else { format!(" · {}", step.name) })),
            dur = format_duration(Some(step.seconds())));
    }
    format!(
        "<section id='route'><div class='sectionhead'><h2>Route</h2>\
         <button class='ghost' id='copyroute' data-text='{}'>Copy route as text</button></div>\
         <div class='metro'>{line}</div></section>",
        esc(&route_text(trace)))
}

/// The route in the "in X: reason → Y" shape, for pasting into tickets.
pub fn route_text(trace: &CallTrace) -> String {
    let mut lines = vec![format!("{} calls {}", trace.caller, trace.dialed)];
    for pair in trace.steps.windows(2) {
        let (previous, step) = (&pair[0], &pair[1]);
        let reason = step.arrival.as_ref().map_or("→".to_string(), |a| a.reason.clone());
        let name = if step.name.is_empty() { String::new() } else { format!(" ({})", step.name) };
        lines.push(format!("in {}: {reason} → {}{name}", previous.number, step.number));
    }
    lines.join("\n")
}

fn time_bar(trace: &CallTrace) -> String {
    let total = trace.seconds().max(0.001);
    let segments: String = trace.steps.iter()
        .map(|step| {
            let share = (step.seconds() / total * 100.0).max(0.6);
            let label = format!("{} · {} · {}", step.number, step.label(), format_duration(Some(step.seconds())));
            let text = if share > 9.0 { esc(&step.label()) } else { String::new() };
            format!("<a class='seg k-{}' href='#step-{}' style='flex-basis:{share:.2}%' title='{}'>{text}</a>",
                    esc(&step.kind), step.index, esc(&label))
        })
        .collect();
    format!("<section id='time'><h2>Where the time went</h2><div class='timebar'>{segments}</div>\
             <div class='timeaxis'><span>{}</span><span>{}</span></div></section>", hms(trace.start), hms(trace.end))
}

// ---- steps ----------------------------------------------------------------------------------

fn step_card(trace: &CallTrace, step: &Step) -> String {
    let kind = esc(&step.kind);
    let end = step.end.map_or("?".to_string(), hms);
    let name = if step.name.is_empty() { String::new() } else { format!(" <span class='name'>{}</span>", esc(&step.name)) };
    let mut html = format!(
        "<article class='step k-{kind}' id='step-{index}'><div class='stephead'><span class='idx'>{index}</span><div>\
         <h3>{number} <span class='chip'>{label}</span>{name}</h3><div class='muted small'>{start} → {end} · {dur} · session <code>{session}</code>{channel}</div>\
         </div></div>",
        index = step.index, number = esc(&step.number), label = esc(&step.label()), start = hms(step.start),
        dur = format_duration(Some(step.seconds())), session = esc(&step.session),
        channel = if step.channel.is_empty() { String::new() } else { format!(" · <code>{}</code>", esc(&step.channel)) });
    if let Some(arrival) = &step.arrival {
        let evidence: String = arrival.evidence.iter().map(|e| format!("<li><code>{}</code></li>", esc(e))).collect();
        let list = if evidence.is_empty() { String::new() } else { format!("<ul>{evidence}</ul>") };
        let _ = write!(html, "<div class='arrival'><span class='muted'>Arrived via</span> <b>{}</b>{list}</div>", esc(&arrival.reason));
    }
    let facts: String = step.facts.iter().map(|(k, v)| format!("<dt>{}</dt><dd>{}</dd>", esc(k), esc(v))).collect();
    let activity: String = step.activity.iter().map(activity_row).collect();
    let activity = if activity.is_empty() { "<p class='muted'>nothing notable</p>".to_string() } else { activity };
    let _ = write!(html, "<div class='stepbody'><dl class='facts'>{facts}</dl><div class='activity'>{activity}</div></div>");
    if let Some(visit) = &step.queue {
        html.push_str(&queue_panel(visit));
    }
    if !step.legs.is_empty() {
        html.push_str(&legs(step));
    }
    let variables: Vec<_> = trace.variables.iter().filter(|v| v.step == step.index).collect();
    if !variables.is_empty() {
        let rows: String = variables.iter()
            .map(|v| format!("<tr><td>{}</td><td><code>{}</code></td><td><code>{}</code></td><td class='muted'>{}</td></tr>",
                             hms(v.time), esc(&v.name), esc(&v.value), esc(&v.source)))
            .collect();
        let _ = write!(html, "<details><summary>Variables set here ({})</summary><table class='grid'>\
                              <tr><th>time</th><th>name</th><th>value</th><th>by</th></tr>{rows}</table></details>", variables.len());
    }
    if !step.script_log.is_empty() {
        let rows: String = step.script_log.iter()
            .map(|a| format!("<div><span class='t'>{}</span><span class='src'>{}</span>{}</div>", hms(a.time), esc(&a.kind), esc(&a.text)))
            .collect();
        let _ = write!(html, "<details><summary>Script output ({} lines)</summary><div class='mono lines'>{rows}</div></details>",
                       step.script_log.len());
    }
    let _ = write!(html, "<a class='loglink' href='#log' data-step='{}'>Show this step's log lines ↓</a></article>", step.index);
    html
}

fn activity_row(activity: &Activity) -> String {
    let icon = match activity.kind.as_str() {
        "prompt" => "♪", "dtmf" => "#", "say" => "›", "warn" => "!", "error" => "✖", "route" => "↪",
        "queue" => "≡", "dial" => "☎", "hold" => "⏸", _ => "·",
    };
    let count = if activity.count > 1 { format!("<span class='count'>×{}</span>", activity.count) } else { String::new() };
    let text = if activity.kind == "dtmf" {
        format!("pressed <b class='mono'>{}</b>", esc(&activity.text))
    } else {
        esc(&activity.text)
    };
    format!("<div class='act a-{}' title='session {}'><span class='t'>{}</span><span class='ic'>{icon}</span><span class='tx'>{text}{count}</span></div>",
            esc(&activity.kind), esc(&activity.session), hms(activity.time))
}

fn legs(step: &Step) -> String {
    let cards: String = step.legs.iter()
        .map(|leg| {
            let facts = [
                ("origin", leg.origin.clone()), ("call", leg.oneliner.clone()), ("outcome", leg.outcome()),
                ("device", leg.device.clone()), ("edge proxy", leg.proxy.clone()),
                ("rang for", leg.ring_seconds.map(|s| format_duration(Some(s))).unwrap_or_default()),
                ("answered", leg.answered_at.map(hms).unwrap_or_default()),
                ("hung up", leg.hungup_at.map(hms).unwrap_or_default()),
                ("last log line", leg.end.map(hms).unwrap_or_default()),
            ];
            let rows: String = facts.iter().filter(|(_, v)| !v.is_empty())
                .map(|(k, v)| format!("<dt>{k}</dt><dd>{}</dd>", esc(v))).collect();
            let errors: String = leg.errors.iter()
                .map(|e| format!("<div class='act a-error'><span class='t'></span><span class='ic'>✖</span><span class='tx'>{}</span></div>", esc(e)))
                .collect();
            format!("<div class='leg'><h4>Linked session <code>{}</code></h4><dl class='facts'>{rows}</dl>{errors}</div>", esc(&leg.session))
        })
        .collect();
    format!("<div class='legs'>{cards}</div>")
}

fn queue_panel(visit: &QueueVisit) -> String {
    let name = if visit.name.is_empty() { visit.queue.clone() } else { format!("{} ({})", visit.queue, visit.name) };
    let stats = [
        ("queue", name),
        ("waited", visit.wait_ms.map_or("?".into(), |ms| format_duration(Some(ms as f64 / 1000.0)))),
        ("answered by", visit.answered_by.clone()),
        ("left because", visit.leave_reason.clone()),
        ("strategy", visit.strategy.clone()),
        ("rounds with no free agent", visit.no_agent_rounds.to_string()),
        ("agent attempts", visit.attempts.len().to_string()),
    ];
    let rows: String = stats.iter().filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("<dt>{k}</dt><dd>{}</dd>", esc(v))).collect();
    format!("<div class='queue'><dl class='facts'>{rows}</dl>{}</div>", queue_chart(visit))
}

fn queue_chart(visit: &QueueVisit) -> String {
    let points = &visit.positions;
    let Some(start) = visit.entered else { return String::new() };
    if points.len() < 2 {
        return String::new();
    }
    let end = visit.left.unwrap_or(points[points.len() - 1].0);
    let span = ((end - start).num_milliseconds() as f64 / 1000.0).max(1.0);
    let top = points.iter().map(|(_, p)| *p).max().unwrap_or(1).max(2);
    let (width, height, left, bottom) = (640.0, 170.0, 36.0, 22.0);
    let x = |when: NaiveDateTime| left + (when - start).num_milliseconds() as f64 / 1000.0 / span * (width - left - 10.0);
    let y = |position: u32| 10.0 + f64::from(top - position) / f64::from(top - 1) * (height - bottom - 20.0);

    let mut path = format!("M{:.1},{:.1}", x(start), y(points[0].1));
    for (when, position) in points {
        let _ = write!(path, "H{:.1}V{:.1}", x(*when), y(*position));
    }
    let _ = write!(path, "H{:.1}", x(end));
    let attempts: String = visit.attempts.iter()
        .map(|(when, agent)| format!("<line class='attempt' x1='{0:.1}' x2='{0:.1}' y1='6' y2='{1}'><title>ringing agent {2} at {3}</title></line>",
                                     x(*when), height - bottom, esc(agent), hms(*when)))
        .collect();
    let dots: String = points.iter()
        .map(|(when, p)| format!("<circle cx='{:.1}' cy='{:.1}' r='2.5'><title>{} — position {p}</title></circle>", x(*when), y(*p), hms(*when)))
        .collect();
    format!(
        "<figure class='chart'><figcaption>Position in queue (announced)</figcaption>\
         <svg viewBox='0 0 {width} {height}' role='img' aria-label='queue position over time'>\
         <line class='axis' x1='{left}' x2='{ax2}' y1='{ay}' y2='{ay}'/>\
         <text x='{lx}' y='{ty:.1}' text-anchor='end'>{top}</text><text x='{lx}' y='{oy:.1}' text-anchor='end'>1</text>\
         <text x='{left}' y='{by}'>{s}</text><text x='{ax2}' y='{by}' text-anchor='end'>{e}</text>\
         {attempts}<path class='line' d='{path}'/>{dots}</svg></figure>",
        ax2 = width - 10.0, ay = height - bottom, lx = left - 6.0, ty = y(top) + 4.0, oy = y(1) + 4.0,
        by = height - 6.0, s = start.format("%H:%M"), e = end.format("%H:%M"))
}

// ---- other sections -------------------------------------------------------------------------

fn issues(trace: &CallTrace) -> String {
    if trace.issues.is_empty() && trace.outcome.failures.is_empty() {
        return "<p class='muted'>No WARN or ERROR lines.</p>".into();
    }
    let mut rows: String = trace.outcome.failures.iter()
        .map(|f| format!("<tr class='lvl-ERROR'><td><b>SIP</b></td><td></td><td>{}</td><td></td><td></td></tr>", esc(f)))
        .collect();
    for issue in &trace.issues {
        let css = format!("lvl-{}{}", issue.level, if issue.usually_noise { " noise" } else { "" });
        let window = if issue.last == issue.first { hms(issue.first) } else { format!("{}–{}", hms(issue.first), hms(issue.last)) };
        let _ = write!(rows,
            "<tr class='{css}'><td><b>{}</b></td><td>{}</td><td><div>{}</div><div class='muted mono small'>{} · {}</div></td>\
             <td class='mono small'>{window}</td><td class='mono small'>{}</td></tr>",
            esc(&issue.level), issue.count, esc(&issue.sample), esc(&issue.app), esc(&issue.loc), esc(&issue.sessions.join(", ")));
    }
    let noise = trace.issues.iter().filter(|i| i.usually_noise).count();
    let toggle = if noise > 0 {
        format!("<label class='toggle'><input type='checkbox' id='shownoise'> show {noise} kinds that appear on most calls</label>")
    } else {
        String::new()
    };
    format!("{toggle}<table class='grid issues'><tr><th>level</th><th>×</th><th>message</th><th>when</th><th>session</th></tr>{rows}</table>")
}

fn variables(trace: &CallTrace) -> String {
    if trace.variables.is_empty() {
        return "<p class='muted'>No variables seen.</p>".into();
    }
    let rows: String = trace.variables.iter()
        .map(|v| format!("<tr data-step='{}'><td class='mono small'>{}</td><td>{}</td><td><code>{}</code></td><td><code>{}</code></td>\
                          <td class='muted small'>{}</td><td class='mono small'>{}</td><td class='mono small'>{}</td></tr>",
                         v.step, hms(v.time), v.step, esc(&v.name), esc(&v.value), esc(&v.source), esc(&v.channel), esc(&v.session)))
        .collect();
    format!("<input class='filter' id='varfilter' placeholder='Filter variables… (e.g. CALLERID, TV-, accountcode)'>\
             <table class='grid' id='vartable'><tr><th>time</th><th>step</th><th>name</th><th>value</th><th>set by</th><th>channel</th><th>session</th></tr>{rows}</table>")
}

fn sip_ladder(trace: &CallTrace) -> String {
    let messages = &trace.sip;
    if messages.is_empty() {
        return "<p class='muted'>No SIP dialog recorded.</p>".into();
    }
    let mut parties: Vec<&str> = Vec::new();
    for message in messages {
        for party in [message.src.as_str(), message.dst.as_str()] {
            if !parties.contains(&party) {
                parties.push(party);
            }
        }
    }
    let (column, row_height, left, top) = (170.0, 26.0, 92.0, 44.0);
    let width = left + column * parties.len() as f64;
    let height = top + row_height * messages.len() as f64 + 10.0;
    let center = |party: &str| left + column * parties.iter().position(|p| *p == party).unwrap_or(0) as f64 + column / 2.0;
    let mut svg = format!("<svg class='ladder' viewBox='0 0 {width} {height}' width='{width}' height='{height}'>");
    for party in &parties {
        let cx = center(party);
        let _ = write!(svg, "<text class='party' x='{cx:.0}' y='18' text-anchor='middle'>{}</text>\
                             <line class='life' x1='{cx:.0}' x2='{cx:.0}' y1='26' y2='{:.0}'/>", esc(party), height - 4.0);
    }
    for (index, message) in messages.iter().enumerate() {
        let y = top + index as f64 * row_height;
        let (x1, x2) = (center(&message.src), center(&message.dst));
        let first = message.label.chars().next().unwrap_or(' ');
        let kind = if message.label.starts_with("DTMF") { "dtmf" }
                   else if "456".contains(first) { "err" }
                   else if first.is_ascii_digit() { "resp" } else { "req" };
        let direction = if x2 >= x1 { 1.0 } else { -1.0 };
        let _ = write!(svg,
            "<g class='msg m-{kind}' data-i='{index}'><rect x='0' y='{ry}' width='{width}' height='{row_height}'/>\
             <text class='t' x='6' y='{y}'>{clock}</text><line x1='{x1:.0}' x2='{lx2:.0}' y1='{ly}' y2='{ly}'/>\
             <path d='M{x2:.0},{ly} l{ah},-4 v8 z'/><text class='lbl' x='{mx:.0}' y='{ty}' text-anchor='middle'>{label}</text></g>",
            ry = y - 15.0, clock = message.time.format("%H:%M:%S%.3f"), lx2 = x2 - 6.0 * direction, ly = y + 4.0,
            ah = -7.0 * direction, mx = (x1 + x2) / 2.0, ty = y - 1.0, label = esc(&message.label));
    }
    svg.push_str("</svg>");
    let data: Vec<Value> = messages.iter()
        .map(|m| json!({"t": m.time.format("%H:%M:%S%.3f").to_string(), "l": m.label, "f": m.src, "to": m.dst,
                        "s": m.session, "c": m.call_id, "m": m.message}))
        .collect();
    format!("<p class='muted small'>Click a message to see it in full.</p><div class='scrollx'>{svg}</div>\
             <pre id='sipmsg' class='mono sipmsg'>Select a message above.</pre>\
             <script type='application/json' id='sipdata'>{}</script>", embed_json(&Value::Array(data)))
}

fn call_summaries(trace: &CallTrace) -> String {
    if trace.call_summaries.is_empty() {
        return "<p class='muted'>No Call-summary lines.</p>".into();
    }
    let text = |value: Option<&Value>| match value {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    let millis = |value: Option<&Value>| format_duration(Some(value.and_then(Value::as_f64).unwrap_or(0.0) / 1000.0));
    let rows: String = trace.call_summaries.iter()
        .map(|s| {
            let codecs: Vec<String> = s.iter().filter(|(k, _)| k.ends_with("_codec"))
                .map(|(k, v)| format!("{}: {}", k.trim_end_matches("_codec"), text(Some(v)))).collect();
            format!("<tr><td><code>{}</code></td><td class='mono small'>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    esc(&text(s.get("host"))), esc(&text(s.get("session"))), esc(&text(s.get("description"))),
                    esc(&text(s.get("response"))), millis(s.get("duration")), millis(s.get("connected_duration")),
                    esc(&codecs.join(", ")), esc(&text(s.get("errorlogs"))))
        })
        .collect();
    format!("<table class='grid'><tr><th>host</th><th>session</th><th>description</th><th>response</th><th>duration</th>\
             <th>connected</th><th>codecs</th><th>errorlogs</th></tr>{rows}</table>")
}

fn log_explorer(trace: &CallTrace, full_log: bool) -> String {
    let mut rows = Vec::new();
    let mut hidden = 0usize;
    let mut apps: Vec<&str> = Vec::new();
    for entry in &trace.entries {
        if !full_log && trace.is_chatty(entry) {
            hidden += 1;
            continue;
        }
        if !apps.contains(&entry.app.as_str()) {
            apps.push(&entry.app);
        }
        let body: String = entry.body.chars().take(6000).collect();
        rows.push(json!([entry.clock(), entry.session, entry.host, entry.app, entry.level, entry.loc, entry.msg, body,
                         trace.step_at(entry.time), entry.thread]));
    }
    apps.sort_unstable();
    let hidden_note = if hidden > 0 {
        format!(" · {hidden} rows from very chatty locations left out (re-run with <code>--full-log</code>)")
    } else {
        String::new()
    };
    let steps: String = trace.steps.iter()
        .map(|s| format!("<option value='{}'>{}. {} {}</option>", s.index, s.index, esc(&s.number), esc(&s.label())))
        .collect();
    let app_options: String = apps.iter().map(|a| format!("<option>{}</option>", esc(a))).collect();
    format!(
        "<div class='logbar'><input class='filter' id='logfilter' placeholder='Search log… (regex ok)'>\
         <select id='logstep'><option value=''>all steps</option>{steps}</select>\
         <select id='logapp'><option value=''>all apps</option>{app_options}</select>\
         <select id='loglevel'><option value=''>all levels</option><option>WARN</option><option>ERROR</option>\
         <option value='PROBLEM'>WARN + ERROR</option></select></div>\
         <p class='muted small'><span id='logcount'></span> of {total} rows{hidden_note}</p>\
         <div id='logrows' class='mono logrows'></div><button id='logmore' hidden>Show more</button>\
         <script type='application/json' id='logdata'>{data}</script>",
        total = rows.len(), data = embed_json(&Value::Array(rows)))
}

fn section(title: &str, anchor: &str, body: &str) -> String {
    format!("<section id='{anchor}'><h2>{}</h2>{body}</section>", esc(title))
}
