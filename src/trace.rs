//! Reconstruct what happened to a call from its simlog entries.
//!
//! The route is built from the dialplan as tproxy executes it. Every time a channel
//! lands on `<context>,<number>,1` and runs `Macro(<kind>)`, the call has arrived at a
//! new destination (refer = switchboard menu, anumber = user, ivrscript = custom IVR,
//! queue = queue). What happened between two arrivals (a keypress, a redirect and its
//! reason, a script redirect, a queue agent answering) becomes the reason for the hop.
//! Linked sessions (agent legs, forks) are attached as legs.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use chrono::{Local, NaiveDateTime, TimeZone};
use regex::{Captures, Regex};
use serde_json::Value;

use crate::logparse::{Entry, parse_rows};
use crate::source::{Bundle, Session, event_text};

macro_rules! re {
    ($name:ident, $pattern:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).unwrap());
    };
}

re!(PBX_AT, r"^Executing PBX at (?P<ctx>[^,\s]+),(?P<exten>[^,\s]+),(?P<prio>\d+) for (?P<chan>\S+)");
re!(EXEC_APP, r"^(?P<chan>\S+) is executing application '(?P<app>[^']+)'(?: with args '(?P<args>.*?)' \(unsubstituted args: '(?P<unsub>.*)'\)| without arguments)");
re!(POSITION_CHANGE, r"^The application changed the dialplan position to (?P<ctx>[^,]+),(?P<exten>[^,]+),(?P<prio>\d+) from (?P<fctx>[^,]+),(?P<fexten>[^,]+),(?P<fprio>\d+) for (?P<chan>\S+)");
re!(DIGIT, r"^We got a callback that stream '(?P<chan>[^']+)' got the digit '(?P<d>[^']+)'");
re!(MENU_TIMEOUT, r"^(?P<chan>\S+) going to timeout extension");
re!(AGI_DONE, r"^Done executing AGI '(?P<name>[^']+)' on (?P<chan>.+?)\. It took (?P<ms>\d+) ms");
re!(AGI_SAY, r"^From (?P<script>[^:]+): (?P<text>.*)$");
re!(AGI_SAY_LINE_PREFIX, r"^\S+\.agi:\d+ ");
re!(BAG_VAR, r"^Setting bag variable '(?P<k>[^']+)' to (?P<v>.*)$");
re!(REDIRECTING, r"Redirecting (?P<a>\S+) to (?P<b>\S+) on behalf of (?P<c>\S+)");
re!(REDIRECT_REASON, r"^REASON: (?P<r>\S+)");
re!(SCRIPT_REDIRECT, r"^Redirecting caller to (?P<to>\S+) using context (?P<ctx>\S+)");
re!(BLIND_TRANSFER, r"^(?P<by>\S+) blind transferring (?P<a>\S+) to (?P<b>\S+)");
re!(FINAL_TARGET, r"^Final target of (?P<dialed>\S+) is (?P<target>\S+)");
re!(PBX_MODE, r"^Active PBX-mode for (?P<name>.+?): (?P<mode>.+)$");
re!(QUEUE_WITH_NAME, r"^(?P<q>queue_\d+)\((?P<qname>[^)]*)\)$");
re!(QUEUE_ARGS, r"^Arguments: (?P<q>queue_\d+)");
re!(CALLING, r"(?P<a>\+?\d{4,}) calling (?P<b>\+?\d{4,})");
re!(PROFILE, r"^Profile for caller (?P<n>\S+), .*?available=(?P<avail>\w+).*?description=(?P<desc>.*)$");
re!(SIP_POOL, r"belongs to SIP-server pool '(?P<pool>[^']+)'");
re!(AUTO_VAR, r"^(?P<k>[A-Z][A-Z0-9_\-]{2,}) is '(?P<v>[^']*)'$");
re!(CID_AFTER, r"^(?P<k>TVX_[A-Z_]+) after: ?(?P<v>.*)$");
re!(PLAYING, r"^Playing '(?P<f>[^']+)' on (?P<chan>\S+)");
re!(HOLD, r"is putting (?P<who>\S+) (?P<state>on|off) hold");
re!(CALL_SUMMARY, r"^Call-summary: (?P<json>\{.*\})");
re!(STARTING_QUEUE, r"^Starting queue '(?P<q>[^']+)'");
re!(QUEUE_POSITION, r"^Playing position for (?P<who>[^(\s]+)\(\w*\) in (?P<q>\S+) which is (?P<pos>\d+)");
re!(QUEUE_FIRST, r"^QueueEntry (?P<who>\S+) is now first \(pos:(?P<pos>\d+)");
re!(QUEUE_ANSWERED, r" in (?P<q>\S+) got answered after (?P<ms>\d+) ms by (?P<chan>\S+)");
re!(QUEUE_LEFT, r" left (?P<q>\S+) leaveReason: (?P<r>\w+)");
re!(QUEUE_NO_AGENTS, r"^Couldn't find any agents to call");
re!(QUEUE_STRATEGY, r"^Calling agents using strategy: (?P<s>\w+)");
re!(LOCKING_AGENT, r"Locking agent (?P<agent>\S+)");
re!(CALL_ATTEMPT, r"Call attempt from queue (?P<q>\S+) \((?P<qname>[^)]*)\) to agent: (?P<agent>\d+)");
re!(CUSTOM_SCRIPT_CLASS, r"/\S*/customscripts/\S*?(?P<cls>\w+)\.java");
re!(LOCAL_EXTEN, r"^Local/(?P<exten>[^@]+)@");
re!(EXECUTING_CHANNEL, r"^(\S+) is executing");
re!(EVENT_PARTY, r"^(?P<n>\+?\d{4,}) \((?P<name>[^)]+)\) (?P<what>answers the call|hung up)");
re!(EVENT_INCOMING, r"^Incoming call from (?P<from>\S+) to (?P<n>\+?\d{4,}) \((?P<name>[^)]+)\)");
re!(EVENT_CALLING_ORG, r#""?(?P<a>\d{4,}) calling (?P<n>\d{4,}) \((?P<name>[^)]+)\)"#);
re!(EVENT_PROXY_HANGUP, r"^(?P<host>[a-z][\w\-]*) \((?P<n>\d+)\) hung up");
re!(EVENT_COMING_FROM, r"^Coming from (?P<sid>\S+) => (?P<why>.*)$");
re!(EVENT_SENDING_INVITE, r"^Sending INVITE to (?P<proxy>\S+) \((?P<n>\d+)\)");
re!(ONELINER_RING, r"ringing for (?P<s>[\d.]+) s");
re!(CSEQ_INVITE, r"(?m)^CSeq:\s*\d+ INVITE");
re!(SIP_ERROR_CODE, r"^[4-6]\d\d\b");
re!(SIP_TO_USER, r"(?m)^To:.*?sip:([^@;>]+)");
re!(SIP_FROM_USER, r"(?m)^From:.*?sip:([^@;>]+)");
re!(NUMBERISH, r"^\+?\d{3,}$");
re!(SIG_CHANNEL, r"(SIP|Local)/\S+");
re!(SIG_OBJECT, r"@[0-9a-f]{6,}");
re!(SIG_HEX, r"\b[0-9a-f]{8,}\b");
re!(SIG_DIGITS, r"\d+");
re!(DIGITS_PROMPT, r"^digits/\d+$");

/// Countries whose national format is 0 + subscriber number.
const TRUNK_ZERO_CODES: [&str; 7] = ["358", "971", "46", "44", "49", "31", "41"];

const SCRIPT_INFRA_FILES: [&str; 8] = [
    "AGIServer.java", "AGIScriptHandler.java", "CachingScriptCompiler.java", "PerlScript.java",
    "TLBClient.java", "AbstractAccountScript.java", "?", "FilterOutScript.java",
];

/// WARN/ERROR lines that show up on almost every healthy call. Still listed, ranked last.
static USUALLY_NOISE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"Answer is a NoOp", r"Nobody handled the digit", r"from MOH$", r"can only be used on SIP channels",
        r"Will not run initLocutus", r"Unhandled event \w+ on Top-originated", r"We will keep .* alive",
        r"^account=\d+, rm=", r"Not forwarding event", r"Terminating dangling dialog", r"Finding EP with target-IP",
        r"No caller dad", r"Will not start a new recording", r"got handled before the origination thread",
        r"No request handler to handle event HANGUP_PEER", r"Couldn't find any agents to call",
    ]
    .iter()
    .map(|p| Regex::new(p).unwrap())
    .collect()
});

/// Locations so chatty that the log explorer hides them unless asked for the full log.
pub const CHATTY_THRESHOLD: usize = 150;

#[derive(Debug, Clone)]
pub struct Activity {
    pub time: NaiveDateTime,
    pub kind: String,
    pub text: String,
    pub session: String,
    pub count: usize,
}

impl Activity {
    fn new(time: NaiveDateTime, kind: &str, text: impl Into<String>, session: &str) -> Activity {
        Activity { time, kind: kind.to_string(), text: text.into(), session: session.to_string(), count: 1 }
    }
}

#[derive(Debug, Clone)]
pub struct Arrival {
    pub reason: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Leg {
    pub session: String,
    pub origin: String,
    pub target: String,
    pub start: Option<NaiveDateTime>,
    pub end: Option<NaiveDateTime>,
    pub oneliner: String,
    pub answered_at: Option<NaiveDateTime>,
    pub answered_name: String,
    pub hungup_at: Option<NaiveDateTime>,
    pub ring_seconds: Option<f64>,
    pub device: String,
    pub proxy: String,
    pub errors: Vec<String>,
}

impl Leg {
    pub fn outcome(&self) -> String {
        match (&self.answered_at, self.answered_name.is_empty()) {
            (Some(_), false) => format!("answered ({})", self.answered_name),
            (Some(_), true) => "answered".into(),
            (None, _) => "not answered".into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct QueueVisit {
    pub queue: String,
    pub name: String,
    pub entered: Option<NaiveDateTime>,
    pub left: Option<NaiveDateTime>,
    pub leave_reason: String,
    pub wait_ms: Option<u64>,
    pub answered_by: String,
    pub strategy: String,
    pub positions: Vec<(NaiveDateTime, u32)>,
    pub attempts: Vec<(NaiveDateTime, String)>,
    pub no_agent_rounds: usize,
}

#[derive(Debug, Clone)]
pub struct Step {
    pub index: usize,
    pub number: String,
    pub kind: String,
    pub start: NaiveDateTime,
    pub end: Option<NaiveDateTime>,
    pub channel: String,
    pub context: String,
    pub session: String,
    pub name: String,
    pub arrival: Option<Arrival>,
    /// Insertion-ordered key/value facts.
    pub facts: Vec<(String, String)>,
    pub activity: Vec<Activity>,
    pub script_log: Vec<Activity>,
    pub legs: Vec<Leg>,
    pub queue: Option<QueueVisit>,
}

impl Step {
    pub fn label(&self) -> String {
        kind_label(&self.kind)
    }

    pub fn seconds(&self) -> f64 {
        self.end.map_or(0.0, |end| seconds_between(self.start, end))
    }

    pub fn fact(&self, key: &str) -> Option<&str> {
        self.facts.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    fn set_fact(&mut self, key: &str, value: String) {
        match self.facts.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,
            None => self.facts.push((key.to_string(), value)),
        }
    }

    fn default_fact(&mut self, key: &str, value: String) {
        if self.fact(key).is_none() {
            self.facts.push((key.to_string(), value));
        }
    }
}

pub fn kind_label(kind: &str) -> String {
    match kind {
        "refer" => "Switchboard / IVR menu".into(),
        "anumber" => "User".into(),
        "ivrscript" => "IVR script".into(),
        "queue" => "Queue".into(),
        "agent" => "Queue agent".into(),
        "answered" => "Answered leg".into(),
        "voicemail" => "Voicemail".into(),
        "conference" => "Conference".into(),
        "call" => "Call".into(),
        other => other
            .split('_')
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut chars = w.chars();
                chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect())
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

#[derive(Debug, Clone)]
pub struct VarChange {
    pub time: NaiveDateTime,
    pub session: String,
    pub channel: String,
    pub name: String,
    pub value: String,
    pub source: String,
    pub step: usize,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub level: String,
    pub app: String,
    pub loc: String,
    pub sample: String,
    pub count: usize,
    pub first: NaiveDateTime,
    pub last: NaiveDateTime,
    pub sessions: Vec<String>,
    pub usually_noise: bool,
}

#[derive(Debug, Clone)]
pub struct SipMessage {
    pub time: NaiveDateTime,
    pub src: String,
    pub dst: String,
    pub label: String,
    pub message: String,
    pub call_id: String,
    pub session: String,
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub summary: String,
    pub answered_number: String,
    pub answered_name: String,
    pub answered_at: Option<NaiveDateTime>,
    pub hangup_by: String,
    pub hangup_at: Option<NaiveDateTime>,
    pub hangup_detail: String,
    pub failures: Vec<String>,
}

impl Outcome {
    pub fn wait_seconds(&self, start: NaiveDateTime) -> Option<f64> {
        self.answered_at.map(|at| seconds_between(start, at))
    }

    pub fn talk_seconds(&self) -> Option<f64> {
        Some(seconds_between(self.answered_at?, self.hangup_at?))
    }
}

pub struct CallTrace {
    pub bundle: Bundle,
    pub root: String,
    pub entries: Vec<Entry>,
    pub caller: String,
    pub dialed: String,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub steps: Vec<Step>,
    pub variables: Vec<VarChange>,
    pub issues: Vec<Issue>,
    pub sip: Vec<SipMessage>,
    pub outcome: Outcome,
    pub call_summaries: Vec<serde_json::Map<String, Value>>,
    pub chatty_locations: HashMap<(String, String), usize>,
}

impl CallTrace {
    pub fn seconds(&self) -> f64 {
        seconds_between(self.start, self.end)
    }

    /// 1-based index of the step that was active at `when`.
    pub fn step_at(&self, when: NaiveDateTime) -> usize {
        step_index_at(&self.steps, when) + 1
    }

    pub fn is_chatty(&self, entry: &Entry) -> bool {
        entry.level == "INFO"
            && self.chatty_locations.get(&(entry.app.clone(), entry.loc.clone())).copied().unwrap_or(0) > CHATTY_THRESHOLD
    }
}

pub fn build_trace(bundle: Bundle) -> CallTrace {
    TraceBuilder::new(bundle).build()
}

pub fn normalize_number(raw: &str) -> String {
    let number = raw.trim();
    for code in TRUNK_ZERO_CODES {
        for prefix in [format!("00{code}"), format!("+{code}")] {
            if let Some(rest) = number.strip_prefix(prefix.as_str())
                && rest.len() >= 6
            {
                return format!("0{rest}");
            }
        }
    }
    number.to_string()
}

pub fn format_duration(seconds: Option<f64>) -> String {
    let Some(seconds) = seconds else { return "–".into() };
    if seconds < 10.0 {
        return format!("{seconds:.1}s");
    }
    let total = seconds.round() as u64;
    let (hours, minutes, secs) = (total / 3600, total % 3600 / 60, total % 60);
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs:02}s")
    } else {
        format!("{secs}s")
    }
}

pub fn seconds_between(from: NaiveDateTime, to: NaiveDateTime) -> f64 {
    (to - from).num_milliseconds() as f64 / 1000.0
}

fn step_index_at(steps: &[Step], when: NaiveDateTime) -> usize {
    steps.partition_point(|step| step.start <= when).saturating_sub(1)
}

fn clean_agi_text(text: &str) -> String {
    let mut text = text.trim();
    if text.starts_with('"') && text.ends_with(" 0") {
        text = text[..text.len() - 2].trim_end();
    }
    let text = text.trim_matches('"').trim();
    AGI_SAY_LINE_PREFIX.replace(text, "").into_owned()
}

fn signature(message: &str) -> String {
    let s = SIG_CHANNEL.replace_all(message, "<chan>");
    let s = SIG_OBJECT.replace_all(&s, "@<obj>");
    let s = SIG_HEX.replace_all(&s, "<hex>");
    let s = SIG_DIGITS.replace_all(&s, "N");
    s.chars().take(180).collect()
}

fn is_usually_noise(message: &str) -> bool {
    USUALLY_NOISE.iter().any(|pattern| pattern.is_match(message))
}

fn gotoif_result(args: &str) -> String {
    let Some((condition, branches)) = args.split_once('?') else { return args.to_string() };
    let (when_true, when_false) = branches.split_once(':').unwrap_or((branches, ""));
    let result = condition.trim().to_lowercase();
    let target = if result == "true" { when_true } else { when_false };
    format!("{result} → {}", target.trim())
}

fn event_time(day: &str, clock: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(&format!("{day} {clock}"), "%Y-%m-%d %H:%M:%S,%3f").ok()
}

fn named<'a>(captures: &'a Captures, name: &str) -> &'a str {
    captures.name(name).map_or("", |m| m.as_str())
}

type Cause = (u8, String, String);

struct Fact {
    time: NaiveDateTime,
    key: String,
    value: String,
    session: String,
}

struct TraceBuilder {
    bundle: Bundle,
    root: String,
    entries: Vec<Entry>,
    steps: Vec<Step>,
    activities: Vec<Activity>,
    script_lines: Vec<Activity>,
    facts: Vec<Fact>,
    variables: Vec<VarChange>,
    pending_causes: Vec<Cause>,
    pending_redirect: String,
    last_script_say: String,
    pbx_answered: bool,
    pbx_position: HashMap<String, (String, String)>,
    last_digit: Option<(NaiveDateTime, String)>,
    queues: Vec<QueueVisit>,
    profiles: HashMap<String, String>,
    call_summaries: Vec<serde_json::Map<String, Value>>,
    names: HashMap<String, String>,
    org_names: HashMap<String, String>,
}

impl TraceBuilder {
    fn new(bundle: Bundle) -> TraceBuilder {
        let root = bundle.root().sessionid.clone();
        let entries = parse_all(&bundle);
        let (names, org_names) = names_from_events(&bundle);
        TraceBuilder {
            bundle, root, entries, steps: vec![], activities: vec![], script_lines: vec![], facts: vec![],
            variables: vec![], pending_causes: vec![], pending_redirect: String::new(),
            last_script_say: String::new(), pbx_answered: false, pbx_position: HashMap::new(),
            last_digit: None, queues: vec![], profiles: HashMap::new(), call_summaries: vec![],
            names, org_names,
        }
    }

    fn build(mut self) -> CallTrace {
        let entries = std::mem::take(&mut self.entries);
        for entry in &entries {
            self.handle(entry);
        }
        self.entries = entries;

        let legs = self.build_legs();
        self.close_steps();
        self.attach_legs(&legs);
        self.close_steps();
        self.distribute();
        self.attach_queues();
        let sip = self.sip_messages();
        let start = self.entries.first().map(|e| e.time).unwrap_or_else(|| Local::now().naive_local());
        let end = self.entries.last().map_or(start, |e| e.time);
        let (caller, dialed) = self.caller_and_dialed(&sip);
        if self.steps.is_empty() {
            self.steps.push(new_step(1, if dialed.is_empty() { "?" } else { &dialed }, "call", start, "", "", &self.root));
            self.steps[0].end = Some(end);
        }
        let outcome = self.outcome(&legs, &sip);
        let issues = self.issues();
        let chatty_locations = self.chatty();
        CallTrace {
            root: self.root,
            caller,
            dialed,
            start,
            end,
            steps: self.steps,
            variables: self.variables,
            issues,
            sip,
            outcome,
            call_summaries: self.call_summaries,
            chatty_locations,
            entries: self.entries,
            bundle: self.bundle,
        }
    }

    // ---- the pass over all entries -------------------------------------------------------

    fn handle(&mut self, entry: &Entry) {
        let msg = entry.msg.as_str();
        let on_root = entry.session == self.root;

        if entry.app == "scriptserver" {
            self.handle_script_entry(entry);
        }
        if entry.is_problem() && !is_usually_noise(msg) {
            self.activities.push(Activity::new(entry.time, &entry.level.to_lowercase(), msg, &entry.session));
        }

        if let Some(c) = PBX_AT.captures(msg) {
            self.pbx_position.insert(c["chan"].to_string(), (c["ctx"].to_string(), c["exten"].to_string()));
        } else if let Some(c) = EXEC_APP.captures(msg) {
            self.handle_app(entry, &c, on_root);
        } else if let Some(c) = POSITION_CHANGE.captures(msg) {
            self.handle_position_change(entry, &c, on_root);
        } else if let Some(c) = DIGIT.captures(msg) {
            self.last_digit = Some((entry.time, c["d"].to_string()));
            self.activities.push(Activity::new(entry.time, "dtmf", &c["d"], &entry.session));
        } else if MENU_TIMEOUT.is_match(msg) {
            self.activities.push(Activity::new(entry.time, "note", "Menu ran to the end without input → timeout extension", &entry.session));
        } else if let Some(c) = AGI_DONE.captures(msg) {
            self.fact(entry, "scripts", format!("{} ({} ms)", &c["name"], &c["ms"]));
        } else if let Some(c) = AGI_SAY.captures(msg) {
            let script = c["script"].trim().to_string();
            let text = clean_agi_text(&c["text"]);
            self.handle_agi_say(entry, &script, &text, on_root);
        } else if let Some(c) = BAG_VAR.captures(msg) {
            self.var(entry, &c["k"], &c["v"], "bag variable");
        } else if let Some(c) = PLAYING.captures(msg) {
            self.activities.push(Activity::new(entry.time, "prompt", &c["f"], &entry.session));
        } else if let Some(c) = HOLD.captures(msg) {
            let text = format!("{} put {} hold (re-INVITE)", &c["who"], &c["state"]);
            self.activities.push(Activity::new(entry.time, "hold", text, &entry.session));
        } else if let Some(c) = CALL_SUMMARY.captures(msg) {
            if let Ok(Value::Object(mut summary)) = serde_json::from_str::<Value>(&c["json"]) {
                summary.insert("host".into(), Value::String(entry.host.clone()));
                summary.insert("session".into(), Value::String(entry.session.clone()));
                self.call_summaries.push(summary);
            }
        } else if entry.app == "top" {
            self.handle_queue_entry(entry, on_root);
        }
    }

    fn handle_app(&mut self, entry: &Entry, c: &Captures, on_root: bool) {
        let (app, args, unsub, chan) = (&c["app"], named(c, "args"), named(c, "unsub"), &c["chan"]);
        match app.to_lowercase().as_str() {
            "macro" if on_root => {
                let (context, exten) = self.pbx_position.get(chan).cloned().unwrap_or_default();
                if NUMBERISH.is_match(&exten) {
                    let kind = args.split(',').next().unwrap_or("").split('|').next().unwrap_or("");
                    self.arrive(entry, &normalize_number(&exten), kind, chan, &context);
                }
            }
            "set" if args.contains('=') => {
                let (name, value) = args.split_once('=').unwrap();
                self.var(entry, name, value, "Set");
                if name == "CDR(accountcode)" {
                    self.fact(entry, "account", value.to_string());
                }
            }
            "gotoif" => {
                let condition = unsub.split('?').next().unwrap_or("").trim();
                self.var(entry, &format!("if {condition}"), &gotoif_result(args), "GotoIf");
            }
            "dial" => {
                let target = args.split('|').next().unwrap_or("");
                self.activities.push(Activity::new(entry.time, "dial", format!("Dial {target}"), &entry.session));
                if on_root {
                    self.pending_causes.push((1, "dial".into(), format!("Dial {target}")));
                }
            }
            "answer" if on_root && !self.pbx_answered => {
                self.pbx_answered = true;
                self.activities.push(Activity::new(entry.time, "note", "PBX answered the call", &entry.session));
            }
            "waitexten" => {
                let seconds = if args.is_empty() { "?" } else { args };
                self.activities.push(Activity::new(entry.time, "note", format!("Waiting {seconds}s for a keypress"), &entry.session));
            }
            _ => {}
        }
    }

    fn handle_position_change(&mut self, entry: &Entry, c: &Captures, on_root: bool) {
        if !on_root {
            return;
        }
        let Some((digit_time, digit)) = self.last_digit.clone() else { return };
        if c["exten"] == digit && seconds_between(digit_time, entry.time) < 5.0 {
            let detail = format!("Menu choice {digit} in {} → {},{digit}", &c["fctx"], &c["ctx"]);
            self.pending_causes.push((3, format!("keypress {digit}"), detail.clone()));
            self.activities.push(Activity::new(entry.time, "route", detail, &entry.session));
            self.last_digit = None;
        }
    }

    fn handle_agi_say(&mut self, entry: &Entry, script: &str, text: &str, on_root: bool) {
        self.script_lines.push(Activity::new(entry.time, script, text, &entry.session));
        if let Some(c) = REDIRECTING.captures(text) {
            self.pending_redirect = format!("{script}: {} → {} on behalf of {}", &c["a"], &c["b"], &c["c"]);
        } else if let Some(c) = REDIRECT_REASON.captures(text) {
            let detail = if self.pending_redirect.is_empty() { text.to_string() } else { self.pending_redirect.clone() };
            if on_root {
                self.pending_causes.push((2, format!("{} redirect", &c["r"]), detail));
            }
            let note = format!("Redirect ({}): {}", &c["r"], self.pending_redirect);
            self.activities.push(Activity::new(entry.time, "route", note, &entry.session));
        } else if let Some(c) = PBX_MODE.captures(text) {
            if let Some(q) = QUEUE_WITH_NAME.captures(&c["name"]) {
                self.queue(&q["q"]).name = q["qname"].to_string();
                self.fact(entry, "queue", format!("{} ({})", &q["q"], &q["qname"]));
            }
            self.fact(entry, "PBX mode", format!("{}  [{}]", &c["mode"], &c["name"]));
        } else if let Some(c) = QUEUE_ARGS.captures(text) {
            let visit = self.queue(&c["q"]);
            visit.entered.get_or_insert(entry.time);
        } else if let Some(c) = SIP_POOL.captures(text) {
            self.fact(entry, "SIP pool", c["pool"].to_string());
        } else if let Some(c) = AUTO_VAR.captures(text) {
            self.var(entry, &c["k"], &c["v"], script);
        } else if text.starts_with("Queue-version") {
            self.fact(entry, "queue version", text.split_whitespace().last().unwrap_or("").to_string());
        }
    }

    fn handle_script_entry(&mut self, entry: &Entry) {
        let msg = entry.msg.as_str();
        let on_root = entry.session == self.root;
        if let Some(c) = CUSTOM_SCRIPT_CLASS.captures(msg) {
            self.fact(entry, "custom script", c["cls"].to_string());
        }
        if let (Some(c), true) = (SCRIPT_REDIRECT.captures(msg), on_root) {
            let why = if self.last_script_say.is_empty() {
                String::new()
            } else {
                format!(" — after: “{}”", self.last_script_say)
            };
            let detail = format!("{}: redirecting caller to {}{why}", entry.method(), &c["to"]);
            self.pending_causes.push((3, "IVR script redirect".into(), detail));
            let note = format!("Script redirects caller to {}", &c["to"]);
            self.activities.push(Activity::new(entry.time, "route", note, &entry.session));
        } else if let (Some(c), true) = (BLIND_TRANSFER.captures(msg), on_root) {
            self.pending_causes.push((2, format!("blind transfer by {}", &c["by"]), msg.to_string()));
        } else if let Some(c) = FINAL_TARGET.captures(msg) {
            self.fact(entry, "resolved", format!("{} → {}", &c["dialed"], &c["target"]));
        } else if let Some(c) = PROFILE.captures(msg) {
            let available = if &c["avail"] == "true" { "available" } else { "not available" };
            let description = if &c["desc"] == "None" { "no profile" } else { &c["desc"] };
            self.profiles.insert(normalize_number(&c["n"]), format!("{description} ({available})"));
        } else if let Some(c) = CID_AFTER.captures(msg) {
            self.var(entry, &c["k"], &c["v"], "FilterOut");
        }

        if !SCRIPT_INFRA_FILES.contains(&entry.source_file()) && !entry.is_problem() {
            self.activities.push(Activity::new(entry.time, "say", msg, &entry.session));
            if !SCRIPT_REDIRECT.is_match(msg) {
                self.last_script_say = msg.to_string();
            }
        }
        let source = if entry.source_file().is_empty() { "scriptserver" } else { entry.source_file() };
        self.script_lines.push(Activity::new(entry.time, source, msg, &entry.session));
    }

    fn handle_queue_entry(&mut self, entry: &Entry, on_root: bool) {
        let msg = entry.msg.as_str();
        if let Some(c) = STARTING_QUEUE.captures(msg) {
            self.queue(&c["q"]).entered.get_or_insert(entry.time);
            self.activities.push(Activity::new(entry.time, "queue", format!("Entered {}", &c["q"]), &entry.session));
        } else if let Some(c) = QUEUE_POSITION.captures(msg) {
            let position = c["pos"].parse().unwrap_or(0);
            self.queue(&c["q"]).positions.push((entry.time, position));
        } else if QUEUE_FIRST.is_match(msg) {
            self.activities.push(Activity::new(entry.time, "queue", msg, &entry.session));
        } else if QUEUE_NO_AGENTS.is_match(msg) {
            for visit in self.queues.iter_mut().filter(|v| v.left.is_none()) {
                visit.no_agent_rounds += 1;
            }
        } else if let Some(c) = QUEUE_STRATEGY.captures(msg) {
            for visit in self.queues.iter_mut().filter(|v| v.strategy.is_empty()) {
                visit.strategy = c["s"].to_string();
            }
        } else if let Some(c) = QUEUE_ANSWERED.captures(msg) {
            let agent_chan = c["chan"].to_string();
            let answered_by = LOCAL_EXTEN.captures(&agent_chan)
                .map_or_else(|| agent_chan.clone(), |e| normalize_number(&e["exten"]));
            let wait_ms: u64 = c["ms"].parse().unwrap_or(0);
            let visit = self.queue(&c["q"]);
            visit.wait_ms = Some(wait_ms);
            visit.answered_by = answered_by.clone();
            if on_root {
                let waited = format_duration(Some(wait_ms as f64 / 1000.0));
                let detail = format!("{}: answered after {wait_ms} ms by {agent_chan}", &c["q"]);
                self.pending_causes.push((3, format!("answered by queue agent after {waited}"), detail));
                let queue_id = c["q"].to_string();
                self.arrive(entry, &answered_by, "agent", &agent_chan, &queue_id);
            }
        } else if let Some(c) = QUEUE_LEFT.captures(msg) {
            let visit = self.queue(&c["q"]);
            visit.left = Some(entry.time);
            visit.leave_reason = c["r"].to_string();
        } else if let Some(c) = LOCKING_AGENT.captures(msg) {
            let agent = normalize_number(&c["agent"]);
            for visit in self.queues.iter_mut().filter(|v| v.left.is_none()) {
                visit.attempts.push((entry.time, agent.clone()));
            }
            self.activities.push(Activity::new(entry.time, "queue", format!("Ringing agent {agent}"), &entry.session));
        }
    }

    // ---- state helpers -------------------------------------------------------------------

    fn arrive(&mut self, entry: &Entry, number: &str, kind: &str, channel: &str, context: &str) {
        if let Some(current) = self.steps.last()
            && current.number == number
            && current.kind == kind
        {
            return;
        }
        let mut step = new_step(self.steps.len() + 1, number, kind, entry.time, channel, context, &entry.session);
        step.name = self.names.get(number).cloned().unwrap_or_default();
        if let Some(org) = self.org_names.get(number).cloned() {
            self.facts.push(Fact { time: entry.time, key: "customer".into(), value: org, session: entry.session.clone() });
        }
        if self.steps.is_empty() {
            self.pending_causes.clear();
        } else {
            step.arrival = Some(self.consume_causes());
        }
        self.steps.push(step);
    }

    fn consume_causes(&mut self) -> Arrival {
        let causes = std::mem::take(&mut self.pending_causes);
        let Some(top) = causes.iter().map(|(priority, _, _)| *priority).max() else {
            return Arrival { reason: "dialplan jump".into(), evidence: vec![] };
        };
        let reason = causes.iter().rev().find(|(p, _, _)| *p == top).map(|(_, r, _)| r.clone()).unwrap();
        let mut evidence: Vec<String> = Vec::new();
        for (_, _, detail) in causes {
            if !detail.is_empty() && !evidence.contains(&detail) {
                evidence.push(detail);
            }
        }
        Arrival { reason, evidence }
    }

    fn fact(&mut self, entry: &Entry, key: &str, value: String) {
        self.facts.push(Fact { time: entry.time, key: key.to_string(), value, session: entry.session.clone() });
    }

    fn var(&mut self, entry: &Entry, name: &str, value: &str, source: &str) {
        let channel = EXECUTING_CHANNEL.captures(&entry.msg).map(|c| c[1].to_string()).unwrap_or_default();
        self.variables.push(VarChange {
            time: entry.time, session: entry.session.clone(), channel, name: name.to_string(),
            value: value.to_string(), source: source.to_string(), step: 0,
        });
    }

    fn queue(&mut self, queue_id: &str) -> &mut QueueVisit {
        if let Some(index) = self.queues.iter().position(|q| q.queue == queue_id) {
            return &mut self.queues[index];
        }
        self.queues.push(QueueVisit { queue: queue_id.to_string(), ..QueueVisit::default() });
        self.queues.last_mut().unwrap()
    }

    // ---- after the pass ------------------------------------------------------------------

    fn close_steps(&mut self) {
        let end = self.entries.last().map(|e| e.time);
        let starts: Vec<NaiveDateTime> = self.steps.iter().map(|s| s.start).collect();
        for (index, step) in self.steps.iter_mut().enumerate() {
            step.end = starts.get(index + 1).copied().or(end);
        }
    }

    fn distribute(&mut self) {
        if self.steps.is_empty() {
            return;
        }
        for activity in std::mem::take(&mut self.activities) {
            let index = step_index_at(&self.steps, activity.time);
            self.steps[index].activity.push(activity);
        }
        for line in std::mem::take(&mut self.script_lines) {
            let index = step_index_at(&self.steps, line.time);
            self.steps[index].script_log.push(line);
        }
        for variable in &mut self.variables {
            variable.step = step_index_at(&self.steps, variable.time) + 1;
        }
        for fact in std::mem::take(&mut self.facts) {
            // A fact belongs to the latest step of the session that logged it.
            let index = self
                .steps
                .iter()
                .rposition(|s| s.session == fact.session && s.start <= fact.time)
                .unwrap_or_else(|| step_index_at(&self.steps, fact.time));
            let step = &mut self.steps[index];
            if fact.key == "scripts" {
                let joined = match step.fact("scripts") {
                    Some(existing) => format!("{existing}, {}", fact.value),
                    None => fact.value,
                };
                step.set_fact("scripts", joined);
            } else {
                step.default_fact(&fact.key, fact.value);
            }
        }
        for step in &mut self.steps {
            if let Some(profile) = self.profiles.get(&step.number) {
                step.set_fact("profile", profile.clone());
            }
            if !step.context.is_empty() {
                let context = step.context.clone();
                step.default_fact("context", context);
            }
            step.activity = compact(std::mem::take(&mut step.activity));
        }
    }

    fn build_legs(&self) -> Vec<Leg> {
        let mut legs: Vec<Leg> = self
            .bundle
            .sessions
            .iter()
            .filter(|s| s.sessionid != self.root)
            .map(|s| self.leg_from_session(s))
            .collect();
        legs.sort_by_key(|leg| leg.start.unwrap_or(NaiveDateTime::MAX));
        legs
    }

    fn leg_from_session(&self, session: &Session) -> Leg {
        let startdate = session.startdate();
        let day = startdate.get(..10).unwrap_or("");
        let mut times = self.entries.iter().filter(|e| e.session == session.sessionid).map(|e| e.time);
        let start = times.next();
        let end = times.next_back().or(start);
        let mut leg = Leg { session: session.sessionid.clone(), oneliner: session.oneliner(), start, end, ..Leg::default() };
        for event in &session.high_level_events {
            let description = event_text(event, "description").trim();
            let when = event_time(day, event_text(event, "time"));
            if let Some(c) = EVENT_COMING_FROM.captures(description) {
                leg.origin = c["why"].to_string();
                if let Some(attempt) = CALL_ATTEMPT.captures(&c["why"]) {
                    leg.target = normalize_number(&attempt["agent"]);
                }
            } else if let Some(c) = EVENT_PARTY.captures(description) {
                if &c["what"] == "answers the call" {
                    leg.answered_at = when;
                    leg.answered_name = c["name"].to_string();
                    if leg.target.is_empty() {
                        leg.target = normalize_number(&c["n"]);
                    }
                } else if leg.hungup_at.is_none() {
                    leg.hungup_at = when;
                }
            } else if let Some(c) = EVENT_SENDING_INVITE.captures(description) {
                leg.proxy = c["proxy"].to_string();
            } else if event_text(event, "highlight") == "red" {
                leg.errors.push(description.to_string());
            }
        }
        if leg.target.is_empty()
            && let Some(c) = CALLING.captures(&leg.oneliner)
        {
            leg.target = normalize_number(&c["b"]);
        }
        if let Some(c) = ONELINER_RING.captures(&leg.oneliner) {
            leg.ring_seconds = c["s"].parse().ok();
        }
        if let Some(invite) = session.sipdialog.iter().find(|m| event_text(m, "description") == "INVITE") {
            leg.device = event_text(invite, "to").to_string();
        }
        leg
    }

    fn attach_legs(&mut self, legs: &[Leg]) {
        for leg in legs {
            let Some(start) = leg.start else { continue };
            if self.steps.is_empty() {
                continue;
            }
            let spawning = step_index_at(&self.steps, start);
            self.steps[spawning].legs.push(leg.clone());
            if leg.answered_at.is_none() {
                continue;
            }
            for step in self.steps.iter_mut().filter(|s| matches!(s.kind.as_str(), "agent" | "answered") && s.number == leg.target) {
                step.session = leg.session.clone();
                if !leg.answered_name.is_empty() {
                    step.name = leg.answered_name.clone();
                }
                step.context.clear();
                step.set_fact("device", leg.device.clone());
                if !leg.proxy.is_empty() {
                    step.set_fact("edge proxy", leg.proxy.clone());
                }
                if let Some(seconds) = leg.ring_seconds {
                    step.set_fact("rang for", format_duration(Some(seconds)));
                }
            }
        }
        let has_answered_step = self.steps.iter().any(|s| matches!(s.kind.as_str(), "agent" | "answered"));
        if let (Some(leg), false) = (legs.iter().rev().find(|l| l.answered_at.is_some()), has_answered_step) {
            let origin = if leg.origin.is_empty() { "linked session" } else { &leg.origin };
            let mut step = new_step(self.steps.len() + 1, &leg.target, "answered", leg.answered_at.unwrap(),
                                    &leg.device, "", &leg.session);
            step.name = leg.answered_name.clone();
            step.arrival = Some(Arrival { reason: format!("answered ({origin})"), evidence: vec![] });
            self.steps.push(step);
        }
    }

    fn attach_queues(&mut self) {
        for visit in &self.queues {
            if let Some(step) = self
                .steps
                .iter_mut()
                .find(|s| s.kind == "queue" && s.fact("queue").is_none_or(|q| q.contains(&visit.queue)))
            {
                step.queue = Some(visit.clone());
                if step.name.is_empty() {
                    step.name = visit.name.clone();
                }
            }
        }
    }

    fn sip_messages(&self) -> Vec<SipMessage> {
        let mut messages: Vec<SipMessage> = Vec::new();
        for session in &self.bundle.sessions {
            for raw in &session.sipdialog {
                let Some(stamp) = raw.get("time").and_then(Value::as_i64) else { continue };
                let Some(time) = Local.timestamp_millis_opt(stamp).single() else { continue };
                messages.push(SipMessage {
                    time: time.naive_local(),
                    src: or_question(event_text(raw, "from")),
                    dst: or_question(event_text(raw, "to")),
                    label: or_question(event_text(raw, "description")),
                    message: event_text(raw, "message").to_string(),
                    call_id: event_text(raw, "callId").to_string(),
                    session: session.sessionid.clone(),
                });
            }
        }
        messages.sort_by_key(|m| m.time);
        let mut invited = HashSet::new();
        for message in messages.iter_mut().filter(|m| m.label == "INVITE" && !m.call_id.is_empty()) {
            if !invited.insert((message.call_id.clone(), message.src.clone(), message.dst.clone())) {
                message.label = "re-INVITE".into();
            }
        }
        messages
    }

    fn caller_and_dialed(&self, sip: &[SipMessage]) -> (String, String) {
        let root_entries: Vec<&Entry> = self.entries.iter().filter(|e| e.session == self.root).collect();
        let preferred = root_entries.iter().filter(|e| e.msg.starts_with("From "));
        for entry in preferred.chain(root_entries.iter()) {
            if let Some(c) = CALLING.captures(&entry.msg) {
                return (normalize_number(&c["a"]), normalize_number(&c["b"]));
            }
        }
        let root = self.bundle.session(&self.root).unwrap();
        if let Some(c) = CALLING.captures(&root.oneliner()) {
            return (normalize_number(&c["a"]), normalize_number(&c["b"]));
        }
        if let Some(invite) = sip.iter().find(|m| m.label == "INVITE") {
            let user = |re: &Regex| re.captures(&invite.message).map_or("?".to_string(), |c| normalize_number(&c[1]));
            return (user(&SIP_FROM_USER), user(&SIP_TO_USER));
        }
        ("?".into(), "?".into())
    }

    fn outcome(&self, legs: &[Leg], sip: &[SipMessage]) -> Outcome {
        let mut outcome = Outcome::default();
        if let Some(step) = self.steps.iter().rev().find(|s| matches!(s.kind.as_str(), "agent" | "answered")) {
            outcome.answered_number = step.number.clone();
            outcome.answered_name = step.name.clone();
            outcome.answered_at = legs.iter().rev().find_map(|l| l.answered_at).or(Some(step.start));
        }

        let mut hangups: Vec<(Option<NaiveDateTime>, String)> = Vec::new();
        for session in &self.bundle.sessions {
            let startdate = session.startdate();
            let day = startdate.get(..10).unwrap_or("");
            for event in &session.high_level_events {
                let description = event_text(event, "description").trim();
                let when = event_time(day, event_text(event, "time"));
                if let Some(c) = EVENT_PARTY.captures(description) {
                    if &c["what"] == "hung up" {
                        hangups.push((when, format!("{} ({})", normalize_number(&c["n"]), &c["name"])));
                    }
                } else if let Some(c) = EVENT_PROXY_HANGUP.captures(description) {
                    hangups.push((when, format!("{} on behalf of {}", &c["host"], &c["n"])));
                }
            }
        }
        let byes: Vec<&SipMessage> = sip.iter().filter(|m| m.label == "BYE").collect();
        if let Some((at, by)) = hangups.into_iter().min_by_key(|(at, _)| at.unwrap_or(NaiveDateTime::MAX)) {
            outcome.hangup_at = at;
            outcome.hangup_by = by;
        } else if let Some(bye) = byes.first() {
            outcome.hangup_at = Some(bye.time);
            outcome.hangup_by = format!("{} (sent BYE)", bye.src);
        }
        if let Some(bye) = byes.first() {
            outcome.hangup_detail = format!("first BYE: {} → {} at {}", bye.src, bye.dst, bye.time.format("%H:%M:%S"));
        }
        for message in sip {
            if SIP_ERROR_CODE.is_match(&message.label) && CSEQ_INVITE.is_match(&message.message) {
                outcome.failures.push(format!("{} from {} at {}", message.label, message.src, message.time.format("%H:%M:%S")));
            }
        }
        outcome.summary = if !outcome.answered_number.is_empty() {
            let name = if outcome.answered_name.is_empty() { String::new() } else { format!(" ({})", outcome.answered_name) };
            format!("Answered by {}{name}", outcome.answered_number)
        } else if let Some(failure) = outcome.failures.last() {
            format!("Failed — {failure}")
        } else {
            "Not answered by a person".into()
        };
        outcome
    }

    fn issues(&self) -> Vec<Issue> {
        let mut order: Vec<(String, String, String, String)> = Vec::new();
        let mut grouped: HashMap<(String, String, String, String), Vec<&Entry>> = HashMap::new();
        for entry in self.entries.iter().filter(|e| e.is_problem()) {
            let key = (entry.level.clone(), entry.app.clone(), entry.loc.clone(), signature(&entry.msg));
            let bucket = grouped.entry(key.clone()).or_default();
            if bucket.is_empty() {
                order.push(key);
            }
            bucket.push(entry);
        }
        let mut issues: Vec<Issue> = order
            .into_iter()
            .map(|key| {
                let items = &grouped[&key];
                let mut sessions: Vec<String> = items.iter().map(|e| e.session.clone()).collect();
                sessions.sort();
                sessions.dedup();
                Issue {
                    level: key.0.clone(), app: key.1.clone(), loc: key.2.clone(), sample: items[0].msg.clone(),
                    count: items.len(), first: items[0].time, last: items[items.len() - 1].time, sessions,
                    usually_noise: is_usually_noise(&items[0].msg),
                }
            })
            .collect();
        let severity = |level: &str| match level { "ERROR" => 0, "WARN" => 1, _ => 2 };
        issues.sort_by_key(|i| (i.usually_noise, severity(&i.level), i.app != "scriptserver", i.first));
        issues
    }

    fn chatty(&self) -> HashMap<(String, String), usize> {
        let mut counts = HashMap::new();
        for entry in self.entries.iter().filter(|e| e.level == "INFO") {
            *counts.entry((entry.app.clone(), entry.loc.clone())).or_insert(0) += 1;
        }
        counts
    }
}

fn or_question(value: &str) -> String {
    if value.is_empty() { "?".into() } else { value.to_string() }
}

fn new_step(index: usize, number: &str, kind: &str, start: NaiveDateTime, channel: &str, context: &str, session: &str) -> Step {
    Step {
        index, number: number.to_string(), kind: kind.to_string(), start, end: None,
        channel: channel.to_string(), context: context.to_string(), session: session.to_string(),
        name: String::new(), arrival: None, facts: vec![], activity: vec![], script_log: vec![], legs: vec![], queue: None,
    }
}

fn parse_all(bundle: &Bundle) -> Vec<Entry> {
    let mut entries: Vec<Entry> = bundle
        .sessions
        .iter()
        .enumerate()
        .flat_map(|(index, session)| {
            let year = session.startdate().get(..4).and_then(|y| y.parse().ok()).unwrap_or(2026);
            parse_rows(&session.log, &session.sessionid, year, index * 10_000_000)
        })
        .collect();
    entries.sort_by_key(|e| (e.time, e.seq));
    entries
}

fn names_from_events(bundle: &Bundle) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut names = HashMap::new();
    let mut org_names = HashMap::new();
    for session in &bundle.sessions {
        for event in &session.high_level_events {
            let description = event_text(event, "description").trim();
            for (pattern, is_org) in [(&*EVENT_PARTY, false), (&*EVENT_INCOMING, false), (&*EVENT_CALLING_ORG, true)] {
                if let Some(c) = pattern.captures(description) {
                    let target = if is_org { &mut org_names } else { &mut names };
                    target.entry(normalize_number(&c["n"])).or_insert_with(|| c["name"].trim().to_string());
                }
            }
        }
    }
    (names, org_names)
}

fn activity_shape(activity: &Activity) -> String {
    if activity.kind == "prompt" {
        return DIGITS_PROMPT.replace(&activity.text, "digits/N").into_owned();
    }
    signature(&activity.text)
}

/// Merge keypress runs into one line and fold repeating cycles into counts.
///
/// Digits typed within a few seconds of each other become one "pressed 1234#" line even
/// when script output is interleaved. A message whose shape matches one of the last few
/// lines of the same kind (queue announcements, polling loops) bumps that line's count.
fn compact(mut activities: Vec<Activity>) -> Vec<Activity> {
    activities.sort_by_key(|a| a.time);
    let mut merged: Vec<Activity> = Vec::new();
    let mut last_digit_at: Option<NaiveDateTime> = None;
    for activity in activities {
        if activity.kind == "dtmf" {
            let window = merged.len().saturating_sub(6);
            let open_run = merged[window..].iter().rposition(|a| a.kind == "dtmf").map(|i| i + window);
            if let (Some(run), Some(at)) = (open_run, last_digit_at)
                && seconds_between(at, activity.time) < 4.0
            {
                merged[run].text.push_str(&activity.text);
                last_digit_at = Some(activity.time);
                continue;
            }
            last_digit_at = Some(activity.time);
            merged.push(activity);
            continue;
        }
        let shape = activity_shape(&activity);
        let window = merged.len().saturating_sub(4);
        if let Some(repeat) = merged[window..].iter_mut().rev().find(|a| a.kind == activity.kind && activity_shape(a) == shape) {
            repeat.count += 1;
            continue;
        }
        merged.push(activity);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The same summary shape the Python version produced, for parity checks.
    fn summary(trace: &CallTrace) -> Value {
        let t = |time: NaiveDateTime| time.format("%H:%M:%S").to_string();
        let tf = |time: NaiveDateTime| time.format("%H:%M:%S%.6f").to_string();
        let steps: Vec<Value> = trace.steps.iter().map(|s| {
            let facts: serde_json::Map<String, Value> = s.facts.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
            let activity: Vec<Value> = s.activity.iter().map(|a| json!([t(a.time), a.kind, a.text, a.count])).collect();
            json!([s.number, s.kind, s.name, s.session, s.arrival.as_ref().map(|a| a.reason.clone()),
                   s.arrival.as_ref().map_or(vec![], |a| a.evidence.clone()), facts, activity])
        }).collect();
        let queues: Vec<Value> = trace.steps.iter().filter_map(|s| s.queue.as_ref()).map(|q| json!([
            q.queue, q.name, q.wait_ms, q.answered_by, q.leave_reason, q.positions.len(), q.no_agent_rounds, q.attempts.len()
        ])).collect();
        json!({
            "caller": trace.caller, "dialed": trace.dialed, "steps": steps,
            "outcome": [trace.outcome.summary, trace.outcome.hangup_by, trace.outcome.hangup_detail, trace.outcome.failures],
            "issues": trace.issues.iter().map(|i| json!([i.level, i.app, i.loc, i.count, i.usually_noise])).collect::<Vec<_>>(),
            "variables": trace.variables.iter().map(|v| json!([tf(v.time), v.step, v.name, v.value, v.source])).collect::<Vec<_>>(),
            "sip": trace.sip.iter().map(|m| json!([tf(m.time), m.src, m.dst, m.label])).collect::<Vec<_>>(),
            "queues": queues,
            "entries": trace.entries.len(),
        })
    }

    fn assert_parity(bundle_json: &str, expected_json: &str) {
        let trace = build_trace(Bundle::from_json(bundle_json).unwrap());
        let actual = summary(&trace);
        let expected: Value = serde_json::from_str(expected_json).unwrap();
        for key in expected.as_object().unwrap().keys() {
            let (a, e) = (&actual[key], &expected[key]);
            if let (Some(a), Some(e)) = (a.as_array(), e.as_array()) {
                for (index, (a, e)) in a.iter().zip(e).enumerate() {
                    assert_eq!(a, e, "{key}[{index}] differs");
                }
                assert_eq!(a.len(), e.len(), "{key} length differs");
            } else {
                assert_eq!(a, e, "{key} differs");
            }
        }
    }

    #[test]
    fn matches_the_python_version_on_the_synthetic_call() {
        assert_parity(include_str!("../tests/fixtures/synthetic.json"), include_str!("../tests/fixtures/synthetic.expected.json"));
    }

    /// Opt-in snapshot check against a real call kept outside the repository.
    ///
    /// COAT_PARITY_BUNDLE is a `--save-raw` file; COAT_PARITY_EXPECTED is the snapshot. If the
    /// snapshot doesn't exist yet, it is recorded from the current code; later runs compare.
    #[test]
    fn real_call_matches_its_snapshot() {
        let (Ok(bundle), Ok(expected)) = (std::env::var("COAT_PARITY_BUNDLE"), std::env::var("COAT_PARITY_EXPECTED")) else {
            return;
        };
        let bundle_json = std::fs::read_to_string(bundle).unwrap();
        if !std::path::Path::new(&expected).exists() {
            let trace = build_trace(Bundle::from_json(&bundle_json).unwrap());
            std::fs::write(&expected, serde_json::to_string_pretty(&summary(&trace)).unwrap()).unwrap();
            eprintln!("recorded a new snapshot in {expected}");
            return;
        }
        assert_parity(&bundle_json, &std::fs::read_to_string(expected).unwrap());
    }

    #[test]
    fn normalizes_numbers_and_durations() {
        assert_eq!(normalize_number("0046846500401"), "0846500401");
        assert_eq!(normalize_number("+46846500400"), "0846500400");
        assert_eq!(normalize_number("0846500400"), "0846500400");
        assert_eq!(normalize_number("queue_1"), "queue_1");
        assert_eq!(format_duration(Some(4.12)), "4.1s");
        assert_eq!(format_duration(Some(60.0)), "1m 00s");
        assert_eq!(format_duration(Some(1307.4)), "21m 47s");
        assert_eq!(format_duration(Some(3725.0)), "1h 02m");
    }

    #[test]
    fn gotoif_and_agi_text() {
        assert_eq!(gotoif_result("false?30:3"), "false → 3");
        assert_eq!(gotoif_result("true ? 4 : 20"), "true → 4");
        assert_eq!(clean_agi_text(r#""0701740605 calling 0846500400" 0"#), "0701740605 calling 0846500400");
        assert_eq!(clean_agi_text("Redirect.agi:90 REASON: unconditional"), "REASON: unconditional");
    }
}
