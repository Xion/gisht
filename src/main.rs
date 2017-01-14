//!
//! gisht -- Gists in the shell
//!

             extern crate ansi_term;
             extern crate clap;
             extern crate conv;
#[macro_use] extern crate custom_derive;
#[macro_use] extern crate enum_derive;
#[macro_use] extern crate error_derive;
             extern crate git2;
             extern crate hyper;
             extern crate isatty;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate maplit;
             extern crate regex;
             extern crate serde_json;
             extern crate shlex;
             extern crate slog_envlogger;
             extern crate slog_stdlog;
             extern crate slog_stream;
             extern crate time;
#[macro_use] extern crate try_opt;
             extern crate url;
             extern crate webbrowser;

// `slog` must precede `log` in declarations here, because we want to simultenously:
// * use the standard `log` macros (at least for a while)
// * be able to initiaize the slog logger using slog macros like o!()
#[macro_use] extern crate slog;
#[macro_use] extern crate log;
// TODO: when migrating to slog completely, `log` can be removed and order restored


#[macro_use]
mod util;

mod args;
mod commands;
mod ext;
mod gist;
mod hosts;
mod logging;


use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::exit;

use ansi_term::{Colour, Style};

use args::{ArgsError, Command, GistArg, Locality, Options};
use commands::{run_gist, print_binary_path, print_gist, open_gist, show_gist_info};
use gist::Gist;
use hosts::FetchMode;
use util::exitcode;


lazy_static! {
    /// Application / package name, as filled out by Cargo.
    static ref NAME: &'static str = option_env!("CARGO_PKG_NAME").unwrap_or("gisht");

    /// Application version, as filled out by Cargo.
    static ref VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");

    /// Application revision, such as Git SHA.
    static ref REVISION: Option<&'static str> = option_env!("X_CARGO_REVISION");
}

lazy_static! {
    /// User-Agent header that the program uses for all outgoing HTTP requests.
    static ref USER_AGENT: String = match *VERSION {
        Some(version) => format!("{}/{}", *NAME, version),
        None => String::from(*NAME),
    };
}

lazy_static! {
    /// Main application's directory.
    static ref APP_DIR: PathBuf =
        env::home_dir().unwrap_or_else(|| env::temp_dir()).join(".gisht");
    // TODO: use the app_dirs crate to get this in a more portable way

    /// Directory where gist sources are stored.
    ///
    /// Subdirectories are structured by host & the remaining part of gist URI,
    /// e.g. `~/.gisht/gists/gh/Octocat/foo` (a directory) for `gh:Octocat/foo`.
    static ref GISTS_DIR: PathBuf = APP_DIR.join("gists");

    /// Directory where (symbolic) links to gist "binaries" are stored.
    ///
    /// Subdirectories are structured by host & the remaining part of gist URI,
    /// e.g. `~/.gisht/bin/gh/Octocat/foo` (a symlink) for `gh:Octocat/foo`.
    static ref BIN_DIR: PathBuf = APP_DIR.join("bin");
}


fn main() {
    let opts = args::parse().unwrap_or_else(|e| {
        print_args_error(e);
        exit(exitcode::EX_USAGE);
    });

    logging::init(opts.verbosity).unwrap();
    trace!("{} {}{}", *NAME,
        VERSION.map(|v| format!("v{}", v)).unwrap_or_else(|| "<UNKNOWN VERSION>".into()),
        REVISION.map(|r| format!(" ({})", r)).unwrap_or_else(|| "".into()));

    ensure_app_dir(&opts);

    let gist = decode_gist(&opts);
    match opts.command {
        Command::Run => run_gist(&gist, opts.gist_args.as_ref().unwrap()),
        Command::Which => print_binary_path(&gist),
        Command::Print => print_gist(&gist),
        Command::Open => open_gist(&gist),
        Command::Info => show_gist_info(&gist),
    }
}

/// Print an error that may occur while parsing arguments.
fn print_args_error(e: ArgsError) {
    match e {
        ArgsError::Parse(ref e) =>
            // In case of generic parse error,
            // message provided by the clap library will be the usage string.
            writeln!(&mut io::stderr(), "{}", e.message),
        e => {
            let mut msg = "Failed to parse arguments".to_owned();
            if let Some(cause) = e.cause() {
                msg += &format!(": {}", cause);
            }
            writeln!(&mut io::stderr(), "{}", msg)
        },
    }.unwrap();
}


/// Ensure that application directory exists.
/// If it needs to be created, this will be treated as application's first run.
fn ensure_app_dir(opts: &Options) {
    if APP_DIR.exists() {
        trace!("Application directory ({}) already exists, skipping creation.",
            APP_DIR.display());
        return;
    }

    // If the first run is interactive, display a warning about executing untrusted code.
    if isatty::stderr_isatty() && !opts.quiet() {
        trace!("Displaying warning about executing untrusted code...");
        let should_continue = display_warning();
        if !should_continue {
            debug!("Warning not acknowledged -- exiting.");
            exit(2);
        }
        trace!("Warning acknowledged.");
    } else {
        trace!("Quiet/non-interactive run, skipping untrusted code warning.");
    }

    trace!("Creating application directory ({})...", APP_DIR.display());
    if let Err(err) = fs::create_dir_all(&*APP_DIR) {
        error!("Failed to create application directory ({}): {}",
            APP_DIR.display(), err);
        exit(exitcode::EX_OSFILE);
    }
    debug!("Application directory ({}) created successfully.", APP_DIR.display());
}


/// Use command line arguments to obtain a Gist object.
/// This may include fetching a fresh gist from a host, or updating it.
fn decode_gist(opts: &Options) -> Gist {
    let gist = match opts.gist {
        GistArg::Uri(ref uri) => {
            debug!("Gist {} specified as the argument", uri);
            Gist::from_uri(uri.clone())
        },
        GistArg::BrowserUrl(ref url) => {
            debug!("Gist URL `{}` specified as the argument", url);
            let url = url.as_str();
            gist_from_url(url).unwrap_or_else(|| {
                error!("URL doesn't point to any gist service: {}", url);
                exit(exitcode::EX_UNAVAILABLE);
            })
        },
    };

    let is_local = gist.is_local();
    if is_local {
        trace!("Gist {} found among already downloaded gists", gist.uri);
    } else {
        trace!("Gist {} hasn't been downloaded yet", gist.uri);
    }

    // Depending on the locality options, fetch a new or updated version of the gist,
    // or perhaps even error out if it doesn't exist.
    match opts.locality {
        None => {
            debug!("Possibly fetching or updating gist {}...", gist.uri);
            let fetch_mode = if is_local { FetchMode::Auto } else { FetchMode::New };
            if let Err(err) = gist.uri.host().fetch_gist(&gist, fetch_mode) {
                error!("Failed to download/update gist {}: {}", gist.uri, err);
                exit(exitcode::EX_IOERR);
            }
        },
        Some(Locality::Local) => {
            if !is_local {
                error!("Gist {} is not available locally -- exiting.", gist.uri);
                exit(exitcode::EX_NOINPUT);
            }
        },
        Some(Locality::Remote) => {
            debug!("Forcing update of gist {}...", gist.uri);
            if let Err(err) = gist.uri.host().fetch_gist(&gist, FetchMode::Always) {
                error!("Failed to update gist {}: {}", gist.uri, err);
                exit(exitcode::EX_IOERR);
            }
        },
    }

    gist
}

/// Ask each of the known gist hosts if they can resolve this URL into a gist.
fn gist_from_url(url: &str) -> Option<Gist> {
    for (id, host) in &*hosts::HOSTS {
        if let Some(res) = host.resolve_url(url) {
            let gist = res.unwrap_or_else(|err| {
                error!("Failed to download {} gist from a URL ({}): {}",
                    host.name(), url, err);
                exit(exitcode::EX_IOERR);
            });
            trace!("URL `{}` identified as `{}` ({}) gist", url, id, host.name());
            return Some(gist);
        }
    }
    None
}


/// Display warning about executing untrusted code and ask the user to continue.
/// Returns whether the user decided to continue.
fn display_warning() -> bool {
    writeln!(&mut io::stderr(), "{}", format_warning_message()).unwrap();

    write!(&mut io::stderr(), "{}", format_warning_ack_prompt()).unwrap();
    let mut answer = String::with_capacity(YES.len());
    io::stdin().read_line(&mut answer).unwrap();

    answer.trim().to_lowercase() == YES
}

/// Return the formatted warning message, incl. coloring if the terminal supports it.
fn format_warning_message() -> String {
    const PREFIX: &'static str = "WARNING";
    const WARNING: &'static [&'static str] = &[
        "gisht is used to download & run code from remote sources.",
        "",
        "Never run gists that you haven't authored, and/or do not trust.",
        "Doing so is dangerous, and may expose your system to security risks!",
        "",
        "(If you continue, this warning won't be shown again).",
        "",
    ];
    let prefix_style =
        if cfg!(unix) { Colour::Yellow.bold() } else { Style::default() };
    format!("{}: {}", prefix_style.paint(PREFIX), WARNING.join(util::LINESEP))
}

/// Return the formatted prompt for warning acknowledgment.
fn format_warning_ack_prompt() -> String {
    const ACK_PROMPT: &'static str = "Do you wish to continue?";
    if cfg!(unix) {
        format!("{} [{}/{}]: ", Style::new().bold().paint(ACK_PROMPT),
            YES, Colour::Green.paint("N"))
    } else {
        format!("{} [{}/{}]: ", ACK_PROMPT, YES, "N")
    }
}

const YES: &'static str = "y";
