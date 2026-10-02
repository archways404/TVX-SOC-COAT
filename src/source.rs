//! Fetch simlog sessions: paged, unscrubbed by default, following linked sessions.
//!
//! `/downloadsession` silently caps a session at ~20 000 rows, so we use the JSON
//! `/logsearch` endpoint and page with `after_row`. Pages are a fixed 20 001 rows and
//! `after_row=k` starts at row k, so page n can be requested as `after_row=n*20000`
//! (one row of overlap, used to verify the seam) without waiting for page n-1. Simlog
//! spends seconds building each page, so for big sessions we fetch pages in parallel.
//!
//! Linked sessions (queue → agent legs, forks) are found only through the explicit
//! "Creating new call X" / "Coming from X" high-level events. Raw `[LID:x]` tags are
//! ignored: queue logs mention every other caller waiting in the same queue.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;
use std::sync::mpsc;
use std::sync::{Arc, LazyLock};
use std::thread;
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A simlog installation COAT can fetch from.
///
/// The addresses are internal, so they are not in the source code. Official builds get
/// them at build time: the release workflow sets `COAT_SIMLOG_NORDIC` / `COAT_SIMLOG_UAE`
/// from repository secrets. Setting the same variables when running COAT overrides
/// whatever was built in, which is how a developer points a local build at simlog.
pub struct Environment {
    pub name: &'static str,
    pub variable: &'static str,
    built_in: Option<&'static str>,
}

pub const ENVIRONMENTS: [Environment; 2] = [
    Environment { name: "nordic", variable: "COAT_SIMLOG_NORDIC", built_in: option_env!("COAT_SIMLOG_NORDIC") },
    Environment { name: "uae", variable: "COAT_SIMLOG_UAE", built_in: option_env!("COAT_SIMLOG_UAE") },
];

/// Rows per page as simlog serves them today. Only used to guess where later pages
/// start; every guess is verified against the one-row overlap before it's trusted.
const PAGE_ROWS: usize = 20_001;
/// How many pages to keep in flight for a big session.
const PARALLEL_PAGES: usize = 4;
/// A first page slower than this means a big session: start fetching ahead.
const SPECULATE_AFTER: Duration = Duration::from_millis(1500);
const MAX_PAGES: usize = 60;

static SESSION_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_\-]{4,64}$").unwrap());
static LID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[LID:([^\]\s]+)\]").unwrap());
static CHILD_EVENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Creating new call (\S+) =>").unwrap());
static PARENT_EVENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Coming from (\S+) =>").unwrap());
static QUERY_SESSIONID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[?&]sessionid=([^&#\s]+)").unwrap());

/// A failure the user can act on (off VPN, unknown session, bad input).
#[derive(Debug, Clone)]
pub struct CoatError(pub String);

impl fmt::Display for CoatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CoatError {}

pub type Result<T> = std::result::Result<T, CoatError>;

fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(CoatError(message.into()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    #[serde(skip)]
    pub sessionid: String,
    pub log: Vec<String>,
    #[serde(rename = "highLevelEvents")]
    pub high_level_events: Vec<Value>,
    pub sipdialog: Vec<Value>,
    pub meta: Map<String, Value>,
}

impl Session {
    pub fn oneliner(&self) -> String {
        match self.meta.get("oneliner") {
            Some(Value::String(s)) if s != "null" => s.clone(),
            _ => String::new(),
        }
    }

    pub fn startdate(&self) -> String {
        self.meta.get("startdate").and_then(Value::as_str).unwrap_or("").to_string()
    }

    /// (children, parents) this session explicitly links to.
    pub fn linked_sessions(&self) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut children = BTreeSet::new();
        let mut parents = BTreeSet::new();
        for event in &self.high_level_events {
            let description = event_text(event, "description");
            children.extend(CHILD_EVENT.captures_iter(description).map(|c| c[1].to_string()));
            parents.extend(PARENT_EVENT.captures_iter(description).map(|c| c[1].to_string()));
        }
        children.remove(&self.sessionid);
        parents.remove(&self.sessionid);
        (children, parents)
    }
}

pub fn event_text<'a>(event: &'a Value, key: &str) -> &'a str {
    event.get(key).and_then(Value::as_str).unwrap_or("")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub env: String,
    pub base_url: String,
    pub scrubbed: bool,
    pub requested: String,
    #[serde(default)]
    pub links: BTreeMap<String, Vec<String>>,
    #[serde(with = "sessions_by_id")]
    pub sessions: Vec<Session>,
}

impl Bundle {
    pub fn root(&self) -> &Session {
        self.sessions
            .iter()
            .min_by(|a, b| (a.startdate(), &a.sessionid).cmp(&(b.startdate(), &b.sessionid)))
            .expect("a bundle always holds at least one session")
    }

    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.sessionid == id)
    }

    pub fn from_json(text: &str) -> Result<Bundle> {
        serde_json::from_str(text).map_err(|e| CoatError(format!("not a coat --save-raw file: {e}")))
    }
}

/// Sessions serialize as `{ "<id>": {...} }`, the same shape the Python version wrote.
mod sessions_by_id {
    use super::Session;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_json::Map;

    pub fn serialize<S: Serializer>(sessions: &[Session], serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = Map::new();
        for session in sessions {
            map.insert(session.sessionid.clone(), serde_json::to_value(session).map_err(serde::ser::Error::custom)?);
        }
        map.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Session>, D::Error> {
        let map = Map::deserialize(deserializer)?;
        map.into_iter()
            .map(|(id, value)| {
                let mut session: Session = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                session.sessionid = id;
                Ok(session)
            })
            .collect()
    }
}

/// Progress callback. Shared across fetch threads, so it must be thread-safe.
pub type Progress = Arc<dyn Fn(String) + Send + Sync>;

/// Where an environment's simlog lives: the run-time variable if set, else the built-in one.
pub fn base_url(env: &str) -> Result<String> {
    let Some(environment) = ENVIRONMENTS.iter().find(|e| e.name == env) else {
        return err(format!("unknown environment {env:?}"));
    };
    let set = |url: &str| !url.trim().is_empty();
    let configured = std::env::var(environment.variable).ok().filter(|url| set(url))
        .or_else(|| environment.built_in.filter(|url| set(url)).map(str::to_string));
    match configured {
        Some(url) => Ok(url.trim().trim_end_matches('/').to_string()),
        None => err(format!(
            "This copy of COAT doesn't know the address of the {env} simlog. Use an official download, \
             or set {} to the simlog address (see docs/development.md).", environment.variable)),
    }
}

/// Extract (sessionid, environment hint) from a simlog URL, a `[LID:x]` tag or a bare id.
pub fn parse_target(target: &str) -> Result<(String, Option<&'static str>)> {
    let target = target.trim();
    if let Some(lid) = LID.captures(target) {
        return Ok((lid[1].to_string(), None));
    }
    if target.contains("://") || target.starts_with("partner.") || target.starts_with("simlog.") {
        let Some(id) = QUERY_SESSIONID.captures(target) else {
            return err(format!("No sessionid= parameter in {target:?}"));
        };
        let host = target.split("://").last().unwrap_or(target).split(['/', '?']).next().unwrap_or("");
        let env = if host.contains(".ae.") { "uae" } else { "nordic" };
        return Ok((id[1].to_string(), Some(env)));
    }
    if SESSION_ID.is_match(target) {
        return Ok((target.to_string(), None));
    }
    err(format!("Don't know how to read {target:?}. Pass a simlog URL or a session id."))
}

pub struct FetchOptions {
    pub env: Option<String>,
    pub scrub: bool,
    pub follow: bool,
    pub max_sessions: usize,
}

pub fn fetch_bundle(target: &str, options: &FetchOptions, progress: Progress) -> Result<Bundle> {
    let (sessionid, env_hint) = parse_target(target)?;
    let client = Client::new();
    let explicit = options.env.is_some();
    let env = options.env.as_deref().or(env_hint).unwrap_or("nordic");
    let mut base = base_url(env)?;
    let mut chosen_env = env;

    let mut root = fetch_session(&client, &base, &sessionid, options.scrub, progress.clone())?;
    let other = if env == "nordic" { "uae" } else { "nordic" };
    if root.is_none() && !explicit && let Ok(other_base) = base_url(other) {
        progress(format!("{sessionid} not found in {env}, trying {other}"));
        root = fetch_session(&client, &other_base, &sessionid, options.scrub, progress.clone())?;
        base = other_base;
        chosen_env = other;
    }
    let Some(root) = root else {
        return err(format!("Session {sessionid} not found (expired past its TTL, or a typo?)"));
    };

    let mut bundle = Bundle {
        env: chosen_env.to_string(),
        base_url: base.clone(),
        scrubbed: options.scrub,
        requested: sessionid,
        links: BTreeMap::new(),
        sessions: vec![root],
    };
    if !options.follow {
        return Ok(bundle);
    }

    // Breadth-first over linked sessions, one parallel wave per level.
    let mut frontier = vec![0usize];
    while !frontier.is_empty() && bundle.sessions.len() < options.max_sessions {
        let mut wanted: Vec<String> = Vec::new();
        let known: HashSet<String> = bundle.sessions.iter().map(|s| s.sessionid.clone()).collect();
        for &index in &frontier {
            let session = &bundle.sessions[index];
            let (children, parents) = session.linked_sessions();
            bundle.links.insert(session.sessionid.clone(), children.union(&parents).cloned().collect());
            for linked in parents.into_iter().chain(children) {
                if !known.contains(&linked) && !wanted.contains(&linked) {
                    wanted.push(linked);
                }
            }
        }
        wanted.truncate(options.max_sessions - bundle.sessions.len());
        for id in &wanted {
            progress(format!("following linked session {id}"));
        }
        let fetched = parallel_map(wanted, |id| fetch_session(&client, &base, &id, options.scrub, progress.clone()));
        frontier.clear();
        for session in fetched {
            if let Some(session) = session? {
                bundle.sessions.push(session);
                frontier.push(bundle.sessions.len() - 1);
            }
        }
    }
    Ok(bundle)
}

fn parallel_map<T: Send, R: Send>(items: Vec<T>, work: impl Fn(T) -> R + Sync) -> Vec<R> {
    thread::scope(|scope| {
        let handles: Vec<_> = items.into_iter().map(|item| scope.spawn(|| work(item))).collect();
        handles.into_iter().map(|h| h.join().expect("fetch thread panicked")).collect()
    })
}

/// One `/logsearch` response, reduced to what we keep.
#[derive(Debug, Clone, Default)]
pub struct Page {
    pub found: bool,
    pub rows: Vec<String>,
    pub events: Vec<Value>,
    pub sip: Vec<Value>,
    pub meta: Map<String, Value>,
    pub more: bool,
}

/// Fetches one page; `None` means the first page (no `after_row`).
pub type PageFetcher = Arc<dyn Fn(Option<usize>) -> Result<Page> + Send + Sync>;

pub fn fetch_session(client: &Client, base: &str, sessionid: &str, scrub: bool, progress: Progress)
                     -> Result<Option<Session>> {
    let client = client.clone();
    let base = base.to_string();
    let id = sessionid.to_string();
    let fetcher: PageFetcher = Arc::new(move |after_row| client.page(&base, &id, scrub, after_row));
    let pages = fetch_pages(fetcher, sessionid, progress)?;
    Ok(assemble(sessionid, pages))
}

/// Fetch every page of a session. Starts with the first page; if that is slow (a big
/// session) or comes back full, requests later pages in parallel at their predicted
/// offsets. Anything unexpected (a failed or misaligned speculative page) falls back to
/// plain sequential paging, so speculation can only make it faster, never wrong.
pub fn fetch_pages(fetcher: PageFetcher, sessionid: &str, progress: Progress) -> Result<Vec<Page>> {
    let (sender, receiver) = mpsc::channel::<(usize, Result<Page>)>();
    let launch = |index: usize| {
        let fetcher = fetcher.clone();
        let sender = sender.clone();
        thread::spawn(move || {
            let after_row = (index > 0).then(|| index * (PAGE_ROWS - 1));
            let _ = sender.send((index, fetcher(after_row)));
        });
    };

    launch(0);
    let mut launched = 1usize;
    let mut pages: BTreeMap<usize, Page> = BTreeMap::new();
    let mut last_page: Option<usize> = None;
    let mut speculating = false;
    let mut speculation_failed = false;

    loop {
        let have_first = pages.contains_key(&0);
        if speculation_failed && have_first {
            break;
        }
        if let Some(last) = last_page
            && (0..=last).all(|i| pages.contains_key(&i))
        {
            break;
        }
        if launched >= MAX_PAGES && pages.len() >= launched {
            break;
        }
        let (index, page) = if speculating || have_first {
            receiver.recv().map_err(|_| CoatError("fetch threads stopped unexpectedly".into()))?
        } else {
            match receiver.recv_timeout(SPECULATE_AFTER) {
                Ok(message) => message,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    speculating = true;
                    while launched < PARALLEL_PAGES {
                        launch(launched);
                        launched += 1;
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return err("fetch threads stopped unexpectedly"),
            }
        };
        let page = match page {
            Ok(page) => page,
            Err(error) if index == 0 => return Err(error),
            Err(_) => {
                speculation_failed = true;
                continue;
            }
        };
        if !page.more || page.rows.len() < PAGE_ROWS {
            last_page = Some(last_page.map_or(index, |last| last.min(index)));
        } else if index == 0 {
            speculating = true;
        }
        pages.insert(index, page);
        let rows: usize = pages.values().map(|p| p.rows.len()).sum();
        let plural = if pages.len() == 1 { "" } else { "s" };
        progress(format!("{sessionid}: {} page{plural}, {rows} rows", pages.len()));

        if speculating && last_page.is_none() && !speculation_failed {
            let highest = pages.keys().copied().max().unwrap_or(0);
            while launched < (highest + PARALLEL_PAGES).min(MAX_PAGES) {
                launch(launched);
                launched += 1;
            }
        }
    }

    if let Some(last) = last_page {
        pages.retain(|&index, _| index <= last);
    }
    let ordered: Vec<Page> = pages.into_values().collect();
    if !speculation_failed && seams_match(&ordered) {
        return Ok(ordered);
    }
    progress(format!("{sessionid}: parallel pages didn't line up, re-fetching in order"));
    fetch_pages_sequentially(&fetcher, ordered.into_iter().next().unwrap_or_default())
}

/// Every predicted page must start with the previous page's last row.
fn seams_match(pages: &[Page]) -> bool {
    let full = pages.iter().take(pages.len().saturating_sub(1)).all(|p| p.rows.len() == PAGE_ROWS);
    full && pages.windows(2).all(|pair| match (pair[0].rows.last(), pair[1].rows.first()) {
        (Some(last), Some(first)) => last == first,
        (_, None) => true,
        _ => false,
    })
}

fn fetch_pages_sequentially(fetcher: &PageFetcher, first: Page) -> Result<Vec<Page>> {
    let mut total = first.rows.len();
    let mut more = first.more;
    let mut pages = vec![first];
    while more && pages.len() < MAX_PAGES {
        let page = fetcher(Some(total.saturating_sub(1)))?;
        if page.rows.is_empty() {
            break;
        }
        total += page.rows.len() - 1;
        more = page.more;
        pages.push(page);
    }
    Ok(pages)
}

/// Join pages into a session, dropping the one-row overlap at each seam.
fn assemble(sessionid: &str, pages: Vec<Page>) -> Option<Session> {
    if !pages.first().is_some_and(|p| p.found) {
        return None;
    }
    let mut rows: Vec<String> = Vec::new();
    let mut events = Vec::new();
    let mut sip = Vec::new();
    let mut meta = Map::new();
    for page in pages {
        let skip = usize::from(!rows.is_empty() && page.rows.first() == rows.last());
        rows.extend(page.rows.into_iter().skip(skip));
        events.extend(page.events);
        sip.extend(page.sip);
        merge_meta(&mut meta, page.meta);
    }
    if rows.is_empty() {
        return None;
    }
    Some(Session {
        sessionid: sessionid.to_string(),
        log: rows,
        high_level_events: dedupe(events),
        sipdialog: dedupe(sip),
        meta,
    })
}

fn merge_meta(meta: &mut Map<String, Value>, page: Map<String, Value>) {
    for (key, value) in page {
        let empty = matches!(&value, Value::Null) || value == "" || value == "null"
            || value.as_array().is_some_and(|a| a.is_empty());
        if empty || (key == "startdate" && meta.contains_key("startdate")) {
            continue;
        }
        meta.insert(key, value);
    }
}

fn dedupe(items: Vec<Value>) -> Vec<Value> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| {
            let mut key = item.clone();
            if let Some(object) = key.as_object_mut() {
                object.remove("rows");
            }
            seen.insert(key.to_string())
        })
        .collect()
}

/// HTTP access to simlog.
#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
}

impl Client {
    pub fn new() -> Client {
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_global(Some(Duration::from_secs(120)))
            .http_status_as_error(true)
            .build();
        Client { agent: ureq::Agent::new_with_config(config) }
    }

    fn page(&self, base: &str, sessionid: &str, scrub: bool, after_row: Option<usize>) -> Result<Page> {
        let mut url = format!("{base}/logsearch?sessionid={sessionid}&scrub={scrub}");
        if let Some(after_row) = after_row {
            url.push_str(&format!("&after_row={after_row}"));
        }
        let mut response = self
            .agent
            .get(&url)
            .header("Accept", "application/json")
            .call()
            .map_err(|e| describe_http_error(e, &url))?;
        let raw = response
            .body_mut()
            .with_config()
            .limit(512 * 1024 * 1024)
            .read_to_vec()
            .map_err(|e| describe_http_error(e, &url))?;
        let text = String::from_utf8(raw).unwrap_or_else(|e| e.into_bytes().iter().map(|&b| b as char).collect());
        if text.trim_start().starts_with('<') {
            return err("Simlog returned an HTML page instead of JSON. Is the session id right?");
        }
        let value: Value = serde_json::from_str(&text).map_err(|e| CoatError(format!("Bad JSON from simlog: {e}")))?;
        Ok(page_from_json(value))
    }
}

impl Default for Client {
    fn default() -> Self {
        Client::new()
    }
}

pub fn page_from_json(mut value: Value) -> Page {
    let scrubbed = value.get("scrubbed").cloned().unwrap_or(Value::Null);
    let Some(Value::Object(mut session)) = value
        .get_mut("sessions")
        .and_then(Value::as_array_mut)
        .and_then(|sessions| (!sessions.is_empty()).then(|| sessions.swap_remove(0)))
    else {
        return Page::default();
    };
    let mut take_list = |key: &str| match session.remove(key) {
        Some(Value::Array(items)) => items,
        _ => Vec::new(),
    };
    let rows = take_list("log").into_iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    let events = take_list("highLevelEvents");
    let sip = take_list("sipdialog");
    let more = session.get("morehint").and_then(Value::as_bool).unwrap_or(false);
    let mut meta = Map::new();
    for key in ["sessionid", "startdate", "enddate", "oneliner", "ttl", "islongterm", "locutus"] {
        if let Some(value) = session.remove(key) {
            meta.insert(key.to_string(), value);
        }
    }
    meta.insert("scrubbed".into(), scrubbed);
    Page { found: true, rows, events, sip, meta, more }
}

fn describe_http_error(error: ureq::Error, url: &str) -> CoatError {
    match error {
        ureq::Error::StatusCode(code) => CoatError(format!("Simlog answered HTTP {code} for {url}")),
        other => CoatError(format!("Can't reach simlog ({other}). Are you on the VPN?")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn quiet() -> Progress {
        Arc::new(|_| {})
    }

    #[test]
    fn partner_url_is_nordic_and_ae_is_uae() {
        let nordic = parse_target("https://partner.telavox.se/partner2/simlog/index.jsp?sessionid=Ab3dE9x").unwrap();
        assert_eq!(nordic, ("Ab3dE9x".to_string(), Some("nordic")));
        let uae = parse_target("http://simlog.example.ae.internal/?sessionid=abc_1").unwrap();
        assert_eq!(uae, ("abc_1".to_string(), Some("uae")));
    }

    #[test]
    fn lid_tag_bare_id_and_bad_url() {
        assert_eq!(parse_target("see [LID:Qx-Example_12]").unwrap().0, "Qx-Example_12");
        assert_eq!(parse_target("Zz_Test7").unwrap(), ("Zz_Test7".to_string(), None));
        assert!(parse_target("https://partner.telavox.se/partner2/simlog/index.jsp").is_err());
    }

    #[test]
    fn simlog_address_comes_from_the_environment_variable() {
        // SAFETY: tests touching this variable run in this one test only.
        unsafe { std::env::set_var("COAT_SIMLOG_UAE", "http://simlog.example.test/") };
        assert_eq!(base_url("uae").unwrap(), "http://simlog.example.test");
        unsafe { std::env::set_var("COAT_SIMLOG_UAE", "") };
        let built_in = option_env!("COAT_SIMLOG_UAE").filter(|v| !v.is_empty());
        assert_eq!(base_url("uae").is_ok(), built_in.is_some());
        assert!(base_url("mars").is_err());
    }

    #[test]
    fn only_explicit_events_link_sessions() {
        let session = Session {
            sessionid: "A".into(),
            log: vec!["Oct  2 10:00:00 h top: [LID:OTHER_CALLER] in the same queue".into()],
            high_level_events: vec![
                serde_json::json!({"description": "Creating new call KID => attempt"}),
                serde_json::json!({"description": "Coming from DAD => x"}),
            ],
            sipdialog: vec![],
            meta: Map::new(),
        };
        let (children, parents) = session.linked_sessions();
        assert_eq!(children.into_iter().collect::<Vec<_>>(), vec!["KID"]);
        assert_eq!(parents.into_iter().collect::<Vec<_>>(), vec!["DAD"]);
    }

    /// A fake simlog holding `total` rows, served the way the real one pages them.
    fn fake_simlog(total: usize, delay: Duration, calls: Arc<Mutex<Vec<Option<usize>>>>) -> PageFetcher {
        Arc::new(move |after_row| {
            calls.lock().unwrap().push(after_row);
            thread::sleep(delay);
            let start = after_row.unwrap_or(0);
            let end = (start + PAGE_ROWS).min(total);
            let rows: Vec<String> = (start..end.max(start)).map(|i| format!("row {i}")).collect();
            Ok(Page { found: true, more: end < total, rows, ..Page::default() })
        })
    }

    #[test]
    fn failed_speculative_page_falls_back_to_sequential() {
        let calls = Arc::new(Mutex::new(vec![]));
        let total = 2 * PAGE_ROWS + 10;
        let inner = fake_simlog(total, Duration::ZERO, calls);
        let flaky_once = Arc::new(Mutex::new(true));
        let fetcher: PageFetcher = Arc::new(move |after_row| {
            if after_row == Some(PAGE_ROWS - 1) && std::mem::replace(&mut *flaky_once.lock().unwrap(), false) {
                return err("boom");
            }
            inner(after_row)
        });
        let rows = joined(fetch_pages(fetcher, "S", quiet()).unwrap());
        assert_eq!(rows.len(), total);
    }

    fn joined(pages: Vec<Page>) -> Vec<String> {
        assemble("S", pages).map(|s| s.log).unwrap_or_default()
    }

    #[test]
    fn small_session_is_one_request() {
        let calls = Arc::new(Mutex::new(vec![]));
        let pages = fetch_pages(fake_simlog(500, Duration::ZERO, calls.clone()), "S", quiet()).unwrap();
        assert_eq!(joined(pages).len(), 500);
        assert_eq!(*calls.lock().unwrap(), vec![None]);
    }

    #[test]
    fn big_session_is_fetched_in_parallel_without_gaps() {
        let calls = Arc::new(Mutex::new(vec![]));
        let total = 3 * PAGE_ROWS + 534;
        let started = std::time::Instant::now();
        let pages = fetch_pages(fake_simlog(total, Duration::from_millis(1800), calls.clone()), "S", quiet()).unwrap();
        let rows = joined(pages);
        assert_eq!(rows.len(), total, "one-row seams are dropped, nothing is missing");
        assert_eq!(rows.first().unwrap(), "row 0");
        assert_eq!(rows.last().unwrap(), &format!("row {}", total - 1));
        assert!(rows.windows(2).all(|w| w[0] != w[1]));
        assert!(started.elapsed() < Duration::from_millis(1800 * 3), "pages overlapped in time");
    }

    #[test]
    fn exactly_full_pages_still_finish() {
        let calls = Arc::new(Mutex::new(vec![]));
        let total = 2 * (PAGE_ROWS - 1) + 1; // the last page is exactly full and says no more
        let rows = joined(fetch_pages(fake_simlog(total, Duration::ZERO, calls), "S", quiet()).unwrap());
        assert_eq!(rows.len(), total);
    }

    #[test]
    fn page_from_json_keeps_rows_events_and_meta() {
        let page = page_from_json(serde_json::json!({
            "scrubbed": false,
            "sessions": [{"log": ["a", "b"], "highLevelEvents": [{"x": 1}], "sipdialog": [],
                          "morehint": true, "startdate": "2026-10-02 10:00:00", "oneliner": "x calling y"}]
        }));
        assert!(page.found && page.more);
        assert_eq!(page.rows, vec!["a", "b"]);
        assert_eq!(page.meta["startdate"], "2026-10-02 10:00:00");
        assert!(!page_from_json(serde_json::json!({"sessions": []})).found);
    }
}
