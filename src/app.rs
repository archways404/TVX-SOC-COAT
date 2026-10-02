//! Double-click behaviour: `coat` with no arguments opens the web UI in the browser.
//!
//! If COAT is already running it just opens a new tab. Otherwise it starts a copy of
//! itself in the background (no terminal window, no Dock icon) and opens the browser
//! once that copy is listening. The background copy stops via the Quit button in the
//! page, or by itself after a long idle period.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::serve::{self, ServeOptions};

pub const DEFAULT_PORT: u16 = 7171;
/// Ports tried in order if 7171 is taken by something else.
pub const PORT_ATTEMPTS: u16 = 10;
const BACKGROUND_ENV: &str = "COAT_BACKGROUND";
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// A background COAT nobody has used for this long shuts itself down.
const IDLE_EXIT: Duration = Duration::from_secs(12 * 60 * 60);
const STARTUP_WAIT: Duration = Duration::from_secs(10);

pub struct Running {
    pub port: u16,
    pub version: String,
}

impl Running {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }
}

pub fn launch() -> Result<(), String> {
    if std::env::var_os(BACKGROUND_ENV).is_some() {
        return serve::run(ServeOptions {
            port: DEFAULT_PORT,
            try_next_ports: true,
            open: false,
            idle_exit: Some(IDLE_EXIT),
        });
    }

    if let Some(running) = find_running() {
        if running.version == VERSION {
            return opened(&running, "COAT is already running");
        }
        // An older (or newer) build is still running: replace it with this one.
        stop(&running);
    }

    let log = log_path();
    spawn_background(&log)?;
    let deadline = Instant::now() + STARTUP_WAIT;
    while Instant::now() < deadline {
        if let Some(running) = find_running() {
            return opened(&running, "COAT started");
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Err(format!("COAT didn't start within {}s. Details: {}", STARTUP_WAIT.as_secs(), log.display()))
}

fn opened(running: &Running, what: &str) -> Result<(), String> {
    let url = running.url();
    serve::open_browser(&url);
    println!("{what}: {url}");
    println!("It runs in the background. Use the Quit button in the page to stop it.");
    Ok(())
}

/// A COAT instance answering on one of our ports, if any.
pub fn find_running() -> Option<Running> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(600)))
        .http_status_as_error(true)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    (DEFAULT_PORT..DEFAULT_PORT + PORT_ATTEMPTS).find_map(|port| {
        let mut response = agent.get(&format!("http://127.0.0.1:{port}/api/ping")).call().ok()?;
        let body = response.body_mut().read_to_string().ok()?;
        let ping: serde_json::Value = serde_json::from_str(&body).ok()?;
        (ping["app"] == "coat").then(|| Running { port, version: ping["version"].as_str().unwrap_or("").to_string() })
    })
}

fn stop(running: &Running) {
    let config = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(2))).build();
    let agent = ureq::Agent::new_with_config(config);
    let _ = agent.post(&format!("http://127.0.0.1:{}/api/quit", running.port)).send_empty();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && find_running().is_some_and(|r| r.port == running.port) {
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn log_path() -> std::path::PathBuf {
    std::env::temp_dir().join("coat.log")
}

fn spawn_background(log: &std::path::Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("can't find my own executable: {e}"))?;
    let log_file = std::fs::OpenOptions::new().create(true).append(true).open(log)
        .map_err(|e| format!("can't open {}: {e}", log.display()))?;
    let log_err = log_file.try_clone().map_err(|e| e.to_string())?;
    let mut command = Command::new(exe);
    command.env(BACKGROUND_ENV, "1").stdin(Stdio::null()).stdout(log_file).stderr(log_err);
    detach(&mut command);
    command.spawn().map(|_| ()).map_err(|e| format!("couldn't start COAT in the background: {e}"))
}

#[cfg(unix)]
fn detach(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // Own process group: closing the terminal (if any) doesn't take COAT down with it.
    command.process_group(0);
}

#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    // No console window for the background copy.
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}
