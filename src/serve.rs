//! `coat serve`: a small local web UI. Paste a simlog link, watch it fetch, read the report.
//!
//! Binds to 127.0.0.1 only. Unscrubbed reports contain personal data and must not be
//! reachable from the network.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use serde_json::json;
use tiny_http::{Header, Request, Response, Server};

use crate::report::{self, Options, Served};
use crate::ui::{self, esc};
use crate::source::{self, FetchOptions, Progress};
use crate::trace::{build_trace, format_duration};
use crate::update::Updater;

/// A finished trace is reused for this long unless the user asks for a fresh fetch.
const CACHE_FOR: Duration = Duration::from_secs(15 * 60);
const KEEP_JOBS: usize = 30;

#[derive(Clone, PartialEq)]
enum JobState {
    Running,
    Done,
    Failed,
}

struct Job {
    id: usize,
    sessionid: String,
    scrub: bool,
    state: JobState,
    log: Vec<String>,
    error: String,
    html: Option<Arc<String>>,
    started: Instant,
    started_label: String,
    summary: String,
    outcome: String,
}

#[derive(Default)]
struct Jobs {
    next_id: usize,
    jobs: Vec<Job>,
}

type Shared = Arc<Mutex<Jobs>>;

pub struct ServeOptions {
    pub port: u16,
    /// Use the next free port if `port` is taken (double-click mode).
    pub try_next_ports: bool,
    pub open: bool,
    /// Shut down after this long without a request (double-click mode).
    pub idle_exit: Option<Duration>,
}

struct Context {
    port: u16,
    last_seen: Mutex<Instant>,
    updater: Arc<Updater>,
}

pub fn run(options: ServeOptions) -> Result<(), String> {
    let (server, port) = bind(&options)?;
    let url = format!("http://127.0.0.1:{port}/");
    eprintln!("COAT is running at {url}  (Ctrl-C or the Quit button to stop)");
    if options.open {
        open_browser(&url);
    }
    let context = Arc::new(Context { port, last_seen: Mutex::new(Instant::now()), updater: Updater::new() });
    let jobs: Shared = Arc::new(Mutex::new(Jobs::default()));
    {
        // A downloaded update installs itself only when nobody is using COAT.
        let (idle_context, idle_jobs) = (context.clone(), jobs.clone());
        context.updater.run_in_background(move || {
            let busy = idle_jobs.lock().unwrap().jobs.iter().any(|j| j.state == JobState::Running);
            !busy && idle_context.last_seen.lock().unwrap().elapsed() >= crate::update::IDLE_BEFORE_INSTALL
        });
    }
    if let Some(idle) = options.idle_exit {
        let (context, jobs) = (context.clone(), jobs.clone());
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(60));
            let busy = jobs.lock().unwrap().jobs.iter().any(|j| j.state == JobState::Running);
            if !busy && context.last_seen.lock().unwrap().elapsed() > idle {
                eprintln!("COAT: idle for {}h, shutting down", idle.as_secs() / 3600);
                std::process::exit(0);
            }
        });
    }
    for request in server.incoming_requests() {
        let (jobs, context) = (jobs.clone(), context.clone());
        thread::spawn(move || handle(request, &jobs, &context));
    }
    Ok(())
}

fn bind(options: &ServeOptions) -> Result<(Server, u16), String> {
    // Right after an update, the previous version is still letting go of the port: wait for
    // it rather than moving to another port (the open page and the bookmark expect this one).
    if std::env::var_os("COAT_TAKEOVER").is_some() {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(server) = Server::http(("127.0.0.1", options.port)) {
                return Ok((server, options.port));
            }
            thread::sleep(Duration::from_millis(150));
        }
    }
    let attempts = if options.try_next_ports { crate::app::PORT_ATTEMPTS } else { 1 };
    let mut last_error = String::new();
    for port in options.port..options.port.saturating_add(attempts) {
        match Server::http(("127.0.0.1", port)) {
            Ok(server) => return Ok((server, port)),
            Err(error) => last_error = format!("can't listen on 127.0.0.1:{port}: {error}"),
        }
    }
    Err(last_error)
}

pub fn open_browser(url: &str) {
    if std::env::var_os("COAT_NO_BROWSER").is_some() {
        return;
    }
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    if let Err(error) = command.arg(url).spawn() {
        eprintln!("couldn't open a browser ({error}); open {url} yourself");
    }
}

/// Only answer requests addressed to this machine's loopback by name, so a web page
/// on another site can't reach the reports through DNS rebinding.
fn host_is_local(request: &Request, port: u16) -> bool {
    let host = request.headers().iter().find(|h| h.field.equiv("Host")).map(|h| h.value.as_str().to_string());
    host.is_some_and(|host| {
        ["127.0.0.1", "localhost", "[::1]"].iter().any(|name| host == format!("{name}:{port}"))
    })
}

fn handle(request: Request, jobs: &Shared, context: &Context) {
    if !host_is_local(&request, context.port) {
        let _ = request.respond(Response::from_string("COAT only answers on 127.0.0.1").with_status_code(403));
        return;
    }
    *context.last_seen.lock().unwrap() = Instant::now();
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let params = parse_query(query);
    let is_post = *request.method() == tiny_http::Method::Post;
    let response = match path {
        "/" => html(200, landing(context.port)),
        "/api/start" => start(jobs, &params),
        "/api/ping" => json_response(200, json!({ "app": "coat", "version": env!("CARGO_PKG_VERSION"), "port": context.port })),
        "/api/recent" => json_response(200, recent_json(jobs)),
        "/api/update" => json_response(200, context.updater.status_json()),
        "/api/update/check" if is_post => {
            let updater = context.updater.clone();
            thread::spawn(move || updater.check());
            json_response(200, context.updater.status_json())
        }
        "/api/update/install" if is_post => {
            let updater = context.updater.clone();
            thread::spawn(move || updater.install());
            json_response(200, json!({ "ok": true }))
        }
        "/api/update/auto" if is_post => {
            context.updater.set_auto(params.get("on").is_some_and(|v| v == "1" || v == "true"));
            json_response(200, context.updater.status_json())
        }
        "/api/quit" if is_post => {
            thread::spawn(|| {
                thread::sleep(Duration::from_millis(300));
                std::process::exit(0);
            });
            json_response(200, json!({ "ok": true }))
        }
        "/api/quit" => json_response(405, json!({ "error": "use POST" })),
        "/favicon.ico" => Response::from_data(include_bytes!("../packaging/favicon-64.png").to_vec())
            .with_header(header("Content-Type", "image/png")),
        _ if path.starts_with("/api/job/") => job_status(jobs, &path["/api/job/".len()..]),
        _ if path.starts_with("/r/") => report_page(jobs, &path["/r/".len()..]),
        _ => html(404, message_page("Not found", "There's nothing here. <a href='/'>Trace a call</a>")),
    };
    let _ = request.respond(response);
}

fn start(jobs: &Shared, params: &HashMap<String, String>) -> Response<std::io::Cursor<Vec<u8>>> {
    let target = params.get("q").or_else(|| params.get("sessionid")).cloned().unwrap_or_default();
    let scrub = params.get("scrub").is_some_and(|v| v == "1" || v == "true");
    let fresh = params.contains_key("fresh");
    let sessionid = match source::parse_target(&target) {
        Ok((id, _)) => id,
        Err(error) => return json_response(400, json!({ "error": error.to_string() })),
    };

    let mut state = jobs.lock().unwrap();
    if !fresh
        && let Some(job) = state.jobs.iter().rev().find(|j| {
            j.sessionid == sessionid && j.scrub == scrub && j.state != JobState::Failed && j.started.elapsed() < CACHE_FOR
        })
    {
        return json_response(200, json!({ "job": job.id }));
    }
    state.next_id += 1;
    let id = state.next_id;
    state.jobs.push(Job {
        id, sessionid: sessionid.clone(), scrub, state: JobState::Running, log: vec![format!("Fetching {sessionid} …")],
        error: String::new(), html: None, started: Instant::now(), started_label: Local::now().format("%H:%M").to_string(),
        summary: String::new(), outcome: String::new(),
    });
    let overflow = state.jobs.len().saturating_sub(KEEP_JOBS);
    state.jobs.drain(..overflow);
    drop(state);

    let jobs = jobs.clone();
    thread::spawn(move || run_job(&jobs, id, &target, scrub));
    json_response(200, json!({ "job": id }))
}

fn run_job(jobs: &Shared, id: usize, target: &str, scrub: bool) {
    let progress_jobs = jobs.clone();
    let progress: Progress = Arc::new(move |line: String| {
        with_job(&progress_jobs, id, |job| job.log.push(line));
    });
    let options = FetchOptions { env: None, scrub, follow: true, max_sessions: 8 };
    let started = Instant::now();
    let result = source::fetch_bundle(target, &options, progress.clone()).map(|bundle| {
        let fetched = started.elapsed();
        progress(format!("Fetched in {:.1}s, analysing …", fetched.as_secs_f64()));
        let trace = build_trace(bundle);
        let refresh = format!("/?q={}&fresh=1{}", trace.root, if scrub { "&scrub=1" } else { "" });
        let html = report::render(&trace, &Options { full_log: false, served: Some(Served { refresh_url: refresh }) });
        let route = report::route_text(&trace).lines().count();
        let summary = format!("{} → {} · {} hop{}", trace.caller, trace.steps.last().map_or("?", |s| s.number.as_str()),
                              route.saturating_sub(1), if route == 2 { "" } else { "s" });
        (html, summary, trace.outcome.summary.clone(), format_duration(Some(trace.seconds())))
    });
    with_job(jobs, id, |job| match result {
        Ok((html, summary, outcome, duration)) => {
            job.html = Some(Arc::new(html));
            job.summary = summary;
            job.outcome = format!("{outcome} · {duration}");
            job.state = JobState::Done;
        }
        Err(error) => {
            job.error = error.to_string();
            job.state = JobState::Failed;
        }
    });
}

fn recent_json(jobs: &Shared) -> serde_json::Value {
    let state = jobs.lock().unwrap();
    let recent: Vec<serde_json::Value> = state.jobs.iter().rev().filter(|j| j.state == JobState::Done).take(12)
        .map(|j| json!({ "url": format!("/r/{}", j.id), "session": j.sessionid, "summary": j.summary,
                         "outcome": j.outcome, "when": j.started_label, "scrubbed": j.scrub }))
        .collect();
    json!(recent)
}

fn with_job(jobs: &Shared, id: usize, change: impl FnOnce(&mut Job)) {
    if let Some(job) = jobs.lock().unwrap().jobs.iter_mut().find(|j| j.id == id) {
        change(job);
    }
}

fn job_status(jobs: &Shared, id: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let state = jobs.lock().unwrap();
    let Some(job) = id.parse::<usize>().ok().and_then(|id| state.jobs.iter().find(|j| j.id == id)) else {
        return json_response(404, json!({ "error": "unknown job" }));
    };
    let label = match job.state {
        JobState::Running => "running",
        JobState::Done => "done",
        JobState::Failed => "failed",
    };
    json_response(200, json!({
        "state": label, "log": job.log, "error": job.error, "url": format!("/r/{}", job.id),
        "elapsed": job.started.elapsed().as_secs_f64(),
    }))
}

fn report_page(jobs: &Shared, id: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let html_page = {
        let state = jobs.lock().unwrap();
        id.parse::<usize>().ok().and_then(|id| state.jobs.iter().find(|j| j.id == id)).and_then(|j| j.html.clone())
    };
    match html_page {
        Some(page_html) => html(200, page_html.as_str().to_string()),
        None => html(404, message_page("Report gone", "That report is no longer in memory. <a href='/'>Trace the call again</a>")),
    }
}

/// A bookmark that sends the simlog page you're on to COAT.
fn bookmarklet(port: u16) -> String {
    format!(
        "javascript:(()=>{{const m=location.href.match(/[?&]sessionid=([^&#]+)/);\
         if(!m){{alert('COAT: open a simlog session page first.');return}}\
         window.open('http://127.0.0.1:{port}/?sessionid='+m[1],'_blank')}})()")
}

const LANDING_JS: &str = include_str!("assets/landing.js");

/// The start page: paste a link, watch it fetch, and the one-click bookmark.
fn landing(port: u16) -> String {
    let body = format!(
        "<div class='landing' id='trace'><img class='logo' src='{logo}' alt=''><h1>COAT</h1>\
         <p class='tag'>Call overview &amp; timeline: paste a simlog link, get the whole call.</p>\
         <form class='bigsearch' id='f'><input type='text' id='q' autofocus autocomplete='off' spellcheck='false' \
         placeholder='Paste a simlog link or session id'>\
         <button type='submit'>Trace</button></form>\
         <div class='opts'><label><input type='checkbox' id='scrub'> scrub personal data</label>\
         <label><input type='checkbox' id='fresh'> don't use a cached result</label></div>\
         <div class='progress' id='progress'><div class='bar'><i></i></div><pre id='plog'></pre><div class='error' id='perr'></div></div>\
         </div>\
         <div class='helpcard' id='bookmark'><h2>One click from simlog</h2>\
         <p>Drag this button to your browser's bookmarks bar:</p>\
         <p><a class='bookmarklet' href='{bookmarklet}' onclick='event.preventDefault();alert(\"Drag this button to your bookmarks bar, don't click it here.\")'>Open in COAT</a></p>\
         <p class='muted small'>Then, on any simlog page, click <b>Open in COAT</b> in your bookmarks bar. COAT needs to be running: \
         open the COAT app first if it isn't.</p></div>",
        bookmarklet = esc(&bookmarklet(port)), logo = ui::logo_uri());
    let nav = vec![ui::NavGroup {
        label: "Start".into(),
        items: vec![
            nav_link("trace", "New trace", "plus"),
            nav_link("bookmark", "One-click bookmark", "bookmark"),
        ],
    }];
    ui::render(&ui::Page {
        title: "COAT".into(), crumbs: vec!["New trace".into()], header_right: String::new(), nav, body,
        script: LANDING_JS.into(), served: true, root: String::new(),
    })
}

fn nav_link(id: &str, label: &str, icon: &'static str) -> ui::NavItem {
    ui::NavItem { href: format!("#{id}"), label: label.into(), icon, spy: Some(id.into()), badge: None, children: vec![] }
}

/// A plain message page (not found, report gone) in the app's frame.
fn message_page(title: &str, text: &str) -> String {
    ui::render(&ui::Page {
        title: format!("COAT · {title}"), crumbs: vec![title.into()], header_right: String::new(), nav: vec![],
        body: format!("<div class='landing'><h1>{}</h1><p class='tag'>{text}</p></div>", esc(title)),
        script: String::new(), served: true, root: String::new(),
    })
}

fn html(status: u16, body: String) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(body)
        .with_status_code(status)
        .with_header(header("Content-Type", "text/html; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"))
}

fn json_response(status: u16, value: serde_json::Value) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_string(value.to_string())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json"))
        .with_header(header("Cache-Control", "no-store"))
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("static header")
}

pub fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(key), percent_decode(value))
        })
        .collect()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'+', _) => out.push(b' '),
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 2;
            }
            (byte, _) => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_parsing_decodes_urls() {
        let params = parse_query("q=https%3A%2F%2Fpartner.telavox.se%2Fx%3Fsessionid%3DAb3dE9x&scrub=1&fresh");
        assert_eq!(params["q"], "https://partner.telavox.se/x?sessionid=Ab3dE9x");
        assert_eq!(params["scrub"], "1");
        assert!(params.contains_key("fresh"));
        assert_eq!(percent_decode("a+b%2"), "a b%2");
    }
}
