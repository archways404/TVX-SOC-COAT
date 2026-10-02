//! coat: trace a phone call through simlog.
//!
//! - `coat` with no arguments (or double-clicking it) opens the web UI in the browser.
//! - `coat <simlog-url | sessionid | [LID:x]>` prints the route and writes an HTML report.
//! - `coat serve` runs the web UI in the foreground.

mod app;
mod logparse;
mod report;
mod serve;
mod source;
mod term;
mod trace;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use clap::{Args, Parser, Subcommand};

use crate::source::{Bundle, CoatError, FetchOptions, Progress};

#[derive(Parser)]
#[command(name = "coat", version, args_conflicts_with_subcommands = true,
          about = "Trace a phone call through simlog: route, hops and why, variables, errors, SIP.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    trace: TraceArgs,
}

#[derive(Subcommand)]
enum Command {
    /// Run the web UI in the foreground (Ctrl-C to stop)
    Serve {
        #[arg(long, default_value_t = app::DEFAULT_PORT)]
        port: u16,
        /// Open the UI in your browser
        #[arg(long)]
        open: bool,
    },
}

#[derive(Args)]
struct TraceArgs {
    /// simlog URL, session id, or [LID:x]
    target: Option<String>,
    /// Force the simlog environment
    #[arg(long, value_parser = ["nordic", "uae"])]
    env: Option<String>,
    /// Ask simlog to scrub PII (default: unscrubbed)
    #[arg(long)]
    scrub: bool,
    /// Don't fetch linked sessions (agent legs etc.)
    #[arg(long)]
    no_follow: bool,
    /// Cap on sessions to fetch, the root included
    #[arg(long, default_value_t = 8)]
    max_sessions: usize,
    /// HTML report path [default: reports/coat-<session>.html]
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Terminal summary only
    #[arg(long)]
    no_html: bool,
    /// Embed every log row in the report, chatty ones too
    #[arg(long)]
    full_log: bool,
    /// Open the report in a browser when done
    #[arg(long)]
    open: bool,
    /// Save the fetched sessions as JSON for offline re-runs
    #[arg(long, value_name = "FILE")]
    save_raw: Option<PathBuf>,
    /// Render from a --save-raw file instead of fetching
    #[arg(long, value_name = "FILE")]
    input: Option<PathBuf>,
    /// Plain terminal output
    #[arg(long)]
    no_color: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::Serve { port, open }) => {
            serve::run(serve::ServeOptions { port, try_next_ports: false, open, idle_exit: None }).map_err(CoatError)
        }
        None if cli.trace.target.is_none() && cli.trace.input.is_none() => app::launch().map_err(CoatError),
        None => trace_command(&cli.trace),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("coat: {error}");
            ExitCode::from(2)
        }
    }
}

fn trace_command(args: &TraceArgs) -> Result<(), CoatError> {
    let started = Instant::now();
    let bundle = load(args)?;
    let fetched = started.elapsed();
    let trace = trace::build_trace(bundle);
    let analysed = started.elapsed() - fetched;

    let mut report_path = None;
    if !args.no_html {
        let path = args.output.clone().unwrap_or_else(|| PathBuf::from(format!("reports/coat-{}.html", trace.root)));
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| CoatError(format!("can't create {}: {e}", parent.display())))?;
        }
        let html = report::render(&trace, &report::Options { full_log: args.full_log, served: None });
        std::fs::write(&path, html).map_err(|e| CoatError(format!("can't write {}: {e}", path.display())))?;
        report_path = Some(path);
    }
    let shown = report_path.as_ref().map(|p| p.display().to_string());
    println!("{}", term::render(&trace, term::use_color() && !args.no_color, shown.as_deref()));
    eprintln!("\n(fetched in {:.1}s, analysed in {:.2}s)", fetched.as_secs_f64(), analysed.as_secs_f64());
    if let (Some(path), true) = (&report_path, args.open) {
        let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        serve::open_browser(&format!("file://{}", absolute.display()));
    }
    Ok(())
}

fn load(args: &TraceArgs) -> Result<Bundle, CoatError> {
    if let Some(input) = &args.input {
        let text = std::fs::read_to_string(input).map_err(|e| CoatError(format!("can't read {}: {e}", input.display())))?;
        return Bundle::from_json(&text);
    }
    let Some(target) = &args.target else {
        return Err(CoatError("give a simlog URL or session id (or --input bundle.json)".into()));
    };
    eprintln!("Fetching {target} ({}) …", if args.scrub { "scrubbed" } else { "unscrubbed" });
    let progress: Progress = Arc::new(|line| eprintln!("  {line}"));
    let options = FetchOptions { env: args.env.clone(), scrub: args.scrub, follow: !args.no_follow, max_sessions: args.max_sessions };
    let bundle = source::fetch_bundle(target, &options, progress)?;
    if let Some(path) = &args.save_raw {
        let json = serde_json::to_string(&bundle).map_err(|e| CoatError(e.to_string()))?;
        std::fs::write(path, json).map_err(|e| CoatError(format!("can't write {}: {e}", path.display())))?;
        eprintln!("Saved raw sessions to {}", path.display());
    }
    Ok(bundle)
}
