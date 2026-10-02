//! Self-contained HTML report. No external requests, so it's safe to open offline.

use std::fmt::Write;
use std::sync::LazyLock;

use chrono::NaiveDateTime;
use regex::Regex;
use serde_json::{Value, json};

use crate::trace::{Activity, CallTrace, QueueVisit, Step, format_duration};

/// The app icon, compiled in so the standalone report file carries it too.
static FAVICON: LazyLock<String> = LazyLock::new(|| data_uri(include_bytes!("../packaging/favicon-64.png")));
static LOGO: LazyLock<String> = LazyLock::new(|| data_uri(include_bytes!("../packaging/logo-160.png")));

pub fn logo_uri() -> &'static str {
    &LOGO
}

pub fn favicon_uri() -> &'static str {
    &FAVICON
}

fn data_uri(png: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::from("data:image/png;base64,");
    for chunk in png.chunks(3) {
        let bytes = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= chunk.len() { ALPHABET[(n >> shift & 63) as usize] as char } else { '=' });
        }
    }
    out
}

static SHORT_ANSWERED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^answered by queue agent after (.+)$").unwrap());

pub struct Options {
    /// Embed every log row, chatty ones too.
    pub full_log: bool,
    /// Rendered by `coat serve`: adds the "trace another call" box and a refresh link.
    pub served: Option<Served>,
}

pub struct Served {
    pub refresh_url: String,
}

pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
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
    let script = if options.served.is_some() { format!("{REPORT_JS}{}", crate::serve::QUIT_JS) } else { REPORT_JS.to_string() };
    page(&format!("COAT {}", trace.root), &topbar(trace, options.served.as_ref()), &body, &script)
}

pub fn page(title: &str, topbar: &str, body: &str, script: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"robots\" content=\"noindex\">\
         <link rel=\"icon\" type=\"image/png\" href=\"{favicon}\">\
         <title>{}</title><style>{CSS}</style></head><body>{topbar}<main>{body}</main><script>{script}</script></body></html>",
        esc(title),
        favicon = favicon_uri()
    )
}

// ---- top of the page -------------------------------------------------------------------------

fn topbar(trace: &CallTrace, served: Option<&Served>) -> String {
    let bundle = &trace.bundle;
    let scrub = if bundle.scrubbed {
        "<span class='badge ok'>scrubbed</span>".to_string()
    } else {
        "<span class='badge warn' title='Contains personal data. Do not paste or share.'>UNSCRUBBED · personal data</span>".to_string()
    };
    let links: String = [("route", "Route"), ("steps", "Steps"), ("issues", "Issues"), ("variables", "Variables"),
                         ("sip", "SIP"), ("log", "Log")]
        .iter()
        .map(|(anchor, label)| format!("<a href='#{anchor}'>{label}</a>"))
        .collect();
    let search = match served {
        Some(served) => format!(
            "<form class='mini' action='/' method='get'><input name='q' placeholder='Trace another call…' autocomplete='off'></form>\
             <a class='badge' href='{}' title='Fetch this session again from simlog'>↻ refresh</a>\
             <button class='ghost' id='quit' title='Stop COAT'>Quit</button>",
            esc(&served.refresh_url)),
        None => String::new(),
    };
    let home = if served.is_some() { "href='/'" } else { "" };
    format!(
        "<header class='topbar'><a class='brand' {home}><img src='{icon}' alt=''>COAT<span>call overview &amp; timeline</span></a><nav>{links}</nav>\
         <div class='meta'>{search}<code>{}</code><span class='badge'>{}</span>{scrub}</div></header>",
        esc(&trace.root), esc(&bundle.env), icon = favicon_uri())
}

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
        "<section class='hero'><h1><span>{}</span><i>calls</i><span>{}</span></h1>\
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
    format!("<section><h2>Where the time went</h2><div class='timebar'>{segments}</div>\
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

pub const CSS: &str = r#"
:root{--bg:#f7f7f5;--panel:#fff;--ink:#1d1f23;--muted:#6b7079;--line:#e3e3df;--soft:#f0f0ec;
--accent:#3b5bdb;--ok:#2b8a3e;--bad:#c92a2a;--warn:#b35c00;
--k-refer:#7048e8;--k-anumber:#1c7ed6;--k-ivrscript:#e67700;--k-queue:#0c8599;--k-agent:#2b8a3e;--k-other:#495057;
--mono:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;color-scheme:light}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){--bg:#111214;--panel:#1a1c20;--ink:#e6e6e3;--muted:#9aa0a8;
--line:#2c2f35;--soft:#22252a;--accent:#8ea2ff;--ok:#51cf66;--bad:#ff6b6b;--warn:#ffa94d;
--k-refer:#9775fa;--k-anumber:#4dabf7;--k-ivrscript:#ffa94d;--k-queue:#3bc9db;--k-agent:#51cf66;--k-other:#adb5bd;color-scheme:dark}}
:root[data-theme=dark]{--bg:#111214;--panel:#1a1c20;--ink:#e6e6e3;--muted:#9aa0a8;--line:#2c2f35;--soft:#22252a;--accent:#8ea2ff;
--ok:#51cf66;--bad:#ff6b6b;--warn:#ffa94d;--k-refer:#9775fa;--k-anumber:#4dabf7;--k-ivrscript:#ffa94d;--k-queue:#3bc9db;--k-agent:#51cf66;--k-other:#adb5bd;color-scheme:dark}
*{box-sizing:border-box}html{scroll-behavior:smooth}
body{margin:0;background:var(--bg);color:var(--ink);font:14px/1.5 system-ui,-apple-system,"Segoe UI",sans-serif}
main{max-width:1240px;margin:0 auto;padding:0 16px 80px}
code,.mono{font-family:var(--mono);font-size:12.5px}
a{color:var(--accent);text-decoration:none}
.muted{color:var(--muted)}.small{font-size:12px}
h2{font-size:12px;letter-spacing:.09em;text-transform:uppercase;color:var(--muted);margin:34px 0 12px}
h3{margin:0;font-size:17px}h4{margin:0 0 6px;font-size:13px}
button{font:inherit;padding:6px 14px;border-radius:8px;border:1px solid var(--line);background:var(--panel);color:var(--ink);cursor:pointer}
button.ghost{font-size:12px;padding:3px 10px;color:var(--muted)}button.ghost:hover{color:var(--ink)}
.sectionhead{display:flex;align-items:center;justify-content:space-between;gap:12px}.sectionhead h2{margin-bottom:12px}
.topbar{position:sticky;top:0;z-index:5;display:flex;gap:16px;align-items:center;flex-wrap:wrap;padding:9px 16px;
background:color-mix(in srgb,var(--panel) 90%,transparent);backdrop-filter:blur(8px);border-bottom:1px solid var(--line)}
.brand{font-weight:800;letter-spacing:.14em;color:var(--ink);display:flex;align-items:center;gap:8px}.brand img{width:24px;height:24px}
.landing .logo{width:112px;height:112px;margin-bottom:6px;filter:drop-shadow(0 10px 24px rgba(20,30,90,.35))}.brand span{font-weight:400;letter-spacing:0;color:var(--muted);margin-left:8px;font-size:12px}
.topbar nav{display:flex;gap:14px;flex-wrap:wrap}.topbar nav a{color:var(--ink);font-size:13px}
.topbar .meta{margin-left:auto;display:flex;gap:8px;align-items:center;flex-wrap:wrap}
.mini input{width:220px;padding:4px 10px;border-radius:99px;border:1px solid var(--line);background:var(--bg);color:var(--ink);font:inherit;font-size:12.5px}
.badge{font-size:11px;padding:2px 8px;border-radius:99px;background:var(--soft);border:1px solid var(--line);color:var(--ink)}
.badge.warn{color:var(--warn);border-color:var(--warn);font-weight:600}.badge.ok{color:var(--ok)}
.hero{padding:28px 0 0}.hero h1{margin:0;font-size:30px;display:flex;gap:12px;align-items:baseline;flex-wrap:wrap;font-family:var(--mono)}
.hero h1 i{font-style:normal;font-weight:400;color:var(--muted);font-size:17px;font-family:system-ui,sans-serif}
.verdict{font-size:18px;font-weight:600;margin:6px 0 10px}.verdict.ok{color:var(--ok)}.verdict.bad{color:var(--bad)}
.pills{display:flex;flex-wrap:wrap;gap:6px}.pill{background:var(--panel);border:1px solid var(--line);border-radius:99px;padding:3px 10px;font-size:12.5px}
.k-refer{--kc:var(--k-refer)}.k-anumber{--kc:var(--k-anumber)}.k-ivrscript{--kc:var(--k-ivrscript)}.k-queue{--kc:var(--k-queue)}
.k-agent,.k-answered{--kc:var(--k-agent)}
/* the route, on one line */
.metro{display:flex;align-items:flex-start;overflow-x:auto;padding:4px 4px 12px;background:var(--panel);border:1px solid var(--line);border-radius:12px;
padding:14px 18px 16px;scrollbar-width:thin}
.stop{flex:0 1 auto;min-width:88px;max-width:150px;display:flex;flex-direction:column;align-items:center;text-align:center;color:var(--ink);
position:relative;z-index:1;padding:0 2px}
.stop .dot{width:20px;height:20px;border-radius:50%;background:var(--panel);border:5px solid var(--kc,var(--k-other));margin:30px 0 8px;flex:none;transition:transform .15s}
a.stop:hover .dot{transform:scale(1.2)}
.stop b{font-family:var(--mono);font-size:13px;white-space:nowrap}
.stop small{display:block;max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:11.5px;line-height:1.35}
.stop .kind{color:var(--kc,var(--muted));font-weight:600}.stop .nm{color:var(--ink);opacity:.8}.stop .dur{color:var(--muted)}
.stop.caller{--kc:var(--muted)}.stop.caller .dot{border-style:dashed}
.hop{flex:1 1 70px;min-width:92px;position:relative;height:56px}
.hop::before{content:"";position:absolute;left:-46px;right:-46px;top:38px;height:3px;background:var(--line);z-index:0;border-radius:2px}
.hop::after{content:"";position:absolute;right:-2px;top:34px;border:5.5px solid transparent;border-left:7px solid var(--muted);border-right:0}
.hop span{position:absolute;left:0;right:0;top:0;height:32px;display:flex;align-items:flex-end;justify-content:center;text-align:center;
font-size:11.5px;line-height:1.2;color:var(--accent);font-weight:500;padding:0 4px;overflow-wrap:anywhere}
.timebar{display:flex;height:30px;border-radius:8px;overflow:hidden;border:1px solid var(--line);background:var(--panel)}
.seg{background:var(--kc,var(--k-other));color:#fff;font-size:11.5px;display:flex;align-items:center;justify-content:center;
white-space:nowrap;overflow:hidden;border-right:2px solid var(--panel);min-width:4px}
.timeaxis{display:flex;justify-content:space-between;color:var(--muted);font-size:12px;font-family:var(--mono)}
.step{background:var(--panel);border:1px solid var(--line);border-left:4px solid var(--kc,var(--k-other));border-radius:10px;padding:16px;margin:0 0 14px;scroll-margin-top:70px}
.stephead{display:flex;gap:12px;align-items:flex-start}
.idx{flex:none;width:30px;height:30px;border-radius:50%;background:var(--kc,var(--k-other));color:#fff;display:flex;align-items:center;justify-content:center;font-weight:700}
.chip{font-size:11.5px;font-weight:600;color:var(--kc);border:1px solid var(--kc);border-radius:99px;padding:1px 8px;vertical-align:middle}
.stephead .name{font-weight:400;color:var(--muted);font-size:14px}
.arrival{margin:12px 0 0;padding:8px 12px;background:var(--soft);border-radius:8px}.arrival ul{margin:4px 0 0;padding-left:18px}
.stepbody{display:grid;grid-template-columns:minmax(220px,1fr) 2fr;gap:16px;margin-top:12px}
.facts{display:grid;grid-template-columns:auto 1fr;gap:3px 12px;margin:0;font-size:12.5px;align-content:start}
.facts dt{color:var(--muted)}.facts dd{margin:0;word-break:break-word}
.act{display:grid;grid-template-columns:62px 18px 1fr;gap:6px;padding:2px 0;font-size:13px;border-bottom:1px dashed var(--soft)}
.act .t{font-family:var(--mono);font-size:12px;color:var(--muted)}.act .ic{text-align:center;color:var(--muted)}
.act .tx{word-break:break-word}.count{margin-left:6px;font-size:11px;color:var(--muted);background:var(--soft);padding:0 6px;border-radius:99px}
.a-warn .ic,.a-warn .tx{color:var(--warn)}.a-error .ic,.a-error .tx{color:var(--bad);font-weight:600}
.a-route .tx{color:var(--accent)}.a-dtmf .ic{color:var(--accent);font-weight:700}.a-prompt .tx{color:var(--muted)}
details{margin-top:10px}summary{cursor:pointer;color:var(--accent);font-size:13px}
.lines>div{white-space:pre-wrap;word-break:break-word;padding:1px 0}.lines .t{color:var(--muted);margin-right:8px}.lines .src{color:var(--k-ivrscript);margin-right:8px}
.loglink{display:inline-block;margin-top:10px;font-size:12.5px}
.queue{display:grid;grid-template-columns:minmax(220px,1fr) 2fr;gap:16px;margin-top:14px;padding-top:12px;border-top:1px solid var(--line)}
.chart{margin:0}.chart figcaption{font-size:12px;color:var(--muted)}.chart svg{width:100%;height:auto}
.chart .line{fill:none;stroke:var(--k-queue);stroke-width:2}.chart circle{fill:var(--k-queue)}
.chart .axis{stroke:var(--line)}.chart text{fill:var(--muted);font-size:11px;font-family:var(--mono)}.chart .attempt{stroke:var(--k-agent);stroke-dasharray:3 3}
.legs{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:10px;margin-top:14px}
.leg{border:1px solid var(--line);border-radius:8px;padding:10px;background:var(--soft)}
table.grid{width:100%;border-collapse:collapse;font-size:13px;background:var(--panel)}
.grid th{text-align:left;font-weight:600;color:var(--muted);font-size:12px;border-bottom:1px solid var(--line);padding:6px 8px}
.grid td{border-bottom:1px solid var(--soft);padding:5px 8px;vertical-align:top;word-break:break-word}
section>table.grid,#issues table{border:1px solid var(--line);border-radius:8px}
.issues .lvl-ERROR b{color:var(--bad)}.issues .lvl-WARN b{color:var(--warn)}.issues tr.noise{display:none}.issues.shownoise tr.noise{display:table-row;opacity:.6}
.toggle{display:block;margin-bottom:8px;font-size:13px;color:var(--muted)}
.filter{width:100%;max-width:420px;padding:7px 10px;border:1px solid var(--line);border-radius:8px;background:var(--panel);color:var(--ink);margin-bottom:8px;font:inherit}
.scrollx{overflow-x:auto;border:1px solid var(--line);border-radius:8px;background:var(--panel)}
.ladder text{font-size:11.5px;fill:var(--ink)}.ladder .party{font-weight:700;font-family:var(--mono)}.ladder .life{stroke:var(--line);stroke-width:2}
.ladder .msg rect{fill:transparent}.ladder .msg:hover rect,.ladder .msg.sel rect{fill:var(--soft)}.ladder .msg{cursor:pointer}
.ladder .msg line{stroke:var(--accent);stroke-width:1.5}.ladder .msg path{fill:var(--accent)}.ladder .t{fill:var(--muted);font-family:var(--mono);font-size:11px}
.ladder .m-resp line{stroke:var(--ok)}.ladder .m-resp path{fill:var(--ok)}.ladder .m-err line{stroke:var(--bad)}.ladder .m-err path{fill:var(--bad)}
.ladder .m-dtmf line{stroke:var(--muted);stroke-dasharray:4 3}.ladder .m-dtmf path{fill:var(--muted)}
.sipmsg{white-space:pre-wrap;word-break:break-all;background:var(--panel);border:1px solid var(--line);border-radius:8px;padding:10px;max-height:420px;overflow:auto}
.logbar{display:flex;gap:8px;flex-wrap:wrap;align-items:flex-start}.logbar .filter{margin:0}
.logbar select{padding:7px;border:1px solid var(--line);border-radius:8px;background:var(--panel);color:var(--ink);font:inherit;max-width:100%}
.logrows{background:var(--panel);border:1px solid var(--line);border-radius:8px;max-height:70vh;overflow:auto}
.lr{display:grid;grid-template-columns:96px 52px 110px 1fr;gap:8px;padding:2px 8px;border-bottom:1px solid var(--soft);font-size:12px}
.lr .lv-WARN{color:var(--warn)}.lr .lv-ERROR{color:var(--bad);font-weight:700}.lr .ap{color:var(--muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.lr .m{word-break:break-word;white-space:pre-wrap}.lr .loc{color:var(--muted)}.lr.hasbody{cursor:pointer}.lr .body{grid-column:1/-1;white-space:pre-wrap;color:var(--muted);display:none}
.lr.open .body{display:block}
#logmore{margin-top:8px}
@media (max-width:820px){.stepbody,.queue{grid-template-columns:1fr}.lr{grid-template-columns:80px 1fr}.lr .lv,.lr .ap{display:none}.mini input{width:150px}}
/* landing page */
.landing{max-width:760px;margin:12vh auto 0;text-align:center}
.landing h1{font-size:44px;letter-spacing:.16em;margin:0}.landing .tag{color:var(--muted);margin:4px 0 28px}
.bigsearch{display:flex;gap:8px;background:var(--panel);border:1px solid var(--line);border-radius:14px;padding:8px;box-shadow:0 8px 30px rgba(0,0,0,.08)}
.bigsearch input[type=text]{flex:1;min-width:0;border:0;background:transparent;color:var(--ink);font:inherit;font-size:16px;padding:8px 10px;outline:none}
.bigsearch button{background:var(--accent);color:#fff;border:0;padding:8px 20px;border-radius:10px;font-weight:600}
.opts{display:flex;justify-content:center;gap:16px;margin-top:12px;font-size:13px;color:var(--muted)}
.progress{margin:22px auto 0;text-align:left;max-width:760px;display:none}.progress.on{display:block}
.progress .bar{height:3px;border-radius:2px;background:var(--soft);overflow:hidden}.progress .bar i{display:block;height:100%;width:30%;background:var(--accent);animation:slide 1.1s infinite ease-in-out}
@keyframes slide{0%{margin-left:-30%}100%{margin-left:100%}}
.progress pre{font-family:var(--mono);font-size:12px;color:var(--muted);margin:10px 0 0;white-space:pre-wrap}
.progress .error{color:var(--bad);font-weight:600;margin-top:10px}
.recent{margin:46px auto 0;max-width:760px;text-align:left}.recent a{display:flex;gap:12px;align-items:baseline;padding:9px 12px;border:1px solid var(--line);
background:var(--panel);border-radius:10px;margin-bottom:6px;color:var(--ink)}.recent a:hover{border-color:var(--accent)}
.corner{position:fixed;top:12px;right:16px}
.helpcard{margin:46px auto 0;max-width:760px;text-align:left;border:1px dashed var(--line);border-radius:12px;padding:4px 18px 10px;background:var(--panel)}
.helpcard p{margin:8px 0}
.bookmarklet{display:inline-block;padding:7px 16px;border-radius:99px;background:var(--accent);color:#fff;font-weight:600;cursor:grab}
.recent code{color:var(--accent)}.recent .when{margin-left:auto;color:var(--muted);font-size:12px;white-space:nowrap}
"#;

const REPORT_JS: &str = r#"
(() => {
  const $ = (s) => document.querySelector(s);
  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));

  const copy = $('#copyroute');
  if (copy) copy.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(copy.dataset.text); copy.textContent = 'Copied ✓'; }
    catch (e) { copy.textContent = 'Copy failed'; }
    setTimeout(() => { copy.textContent = 'Copy route as text'; }, 1600);
  });

  const noise = $('#shownoise');
  if (noise) noise.addEventListener('change', () => $('table.issues').classList.toggle('shownoise', noise.checked));

  const vf = $('#varfilter');
  if (vf) vf.addEventListener('input', () => {
    const q = vf.value.toLowerCase();
    document.querySelectorAll('#vartable tr[data-step]').forEach((tr) => {
      tr.style.display = tr.textContent.toLowerCase().includes(q) ? '' : 'none';
    });
  });

  const sipData = $('#sipdata') ? JSON.parse($('#sipdata').textContent) : [];
  document.querySelectorAll('.ladder .msg').forEach((g) => g.addEventListener('click', () => {
    document.querySelectorAll('.ladder .msg.sel').forEach((x) => x.classList.remove('sel'));
    g.classList.add('sel');
    const m = sipData[+g.dataset.i];
    $('#sipmsg').textContent = `${m.t}  ${m.f} → ${m.to}  [${m.s}]  ${m.c || ''}\n\n${m.m || m.l}`;
  }));

  const logData = $('#logdata') ? JSON.parse($('#logdata').textContent) : [];
  const rowsEl = $('#logrows'), more = $('#logmore'), countEl = $('#logcount');
  const PAGE = 1500;
  let matches = [], shown = 0;
  function rowHtml(r) {
    const [t, sess, host, app, level, loc, msg, body, , thread] = r;
    return `<div class="lr${body ? ' hasbody' : ''}"><span>${esc(t)}</span><span class="lv lv-${esc(level)}">${esc(level)}</span>` +
      `<span class="ap" title="${esc(host)} ${esc(app)} [${esc(sess)}] ${esc(thread || '')}">${esc(app)}</span>` +
      `<span class="m">${esc(msg)} <span class="loc">${esc(loc)}</span></span>` +
      (body ? `<span class="body">${esc(body)}</span>` : '') + `</div>`;
  }
  function renderMore() {
    rowsEl.insertAdjacentHTML('beforeend', matches.slice(shown, shown + PAGE).map(rowHtml).join(''));
    shown = Math.min(shown + PAGE, matches.length);
    more.hidden = shown >= matches.length;
  }
  function applyFilter() {
    const text = $('#logfilter').value, step = $('#logstep').value, app = $('#logapp').value, level = $('#loglevel').value;
    let re = null;
    if (text) { try { re = new RegExp(text, 'i'); } catch (e) { re = null; } }
    const needle = text.toLowerCase();
    matches = logData.filter((r) => {
      if (step && String(r[8]) !== step) return false;
      if (app && r[3] !== app) return false;
      if (level === 'PROBLEM' ? !(r[4] === 'WARN' || r[4] === 'ERROR') : (level && r[4] !== level)) return false;
      if (!text) return true;
      const hay = r[6] + ' ' + r[5] + ' ' + r[7];
      return re ? re.test(hay) : hay.toLowerCase().includes(needle);
    });
    rowsEl.innerHTML = ''; shown = 0; countEl.textContent = matches.length; renderMore();
  }
  if (rowsEl) {
    ['#logfilter', '#logstep', '#logapp', '#loglevel'].forEach((s) => $(s).addEventListener('input', applyFilter));
    more.addEventListener('click', renderMore);
    rowsEl.addEventListener('click', (e) => { const lr = e.target.closest('.lr.hasbody'); if (lr) lr.classList.toggle('open'); });
    document.querySelectorAll('.loglink').forEach((a) => a.addEventListener('click', () => {
      $('#logstep').value = a.dataset.step; applyFilter();
    }));
    applyFilter();
  }
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(data_uri(b"Man"), "data:image/png;base64,TWFu");
        assert_eq!(data_uri(b"Ma"), "data:image/png;base64,TWE=");
        assert_eq!(data_uri(b"M"), "data:image/png;base64,TQ==");
        assert!(favicon_uri().starts_with("data:image/png;base64,iVBORw0KGgo"));
    }
}
