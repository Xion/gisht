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
             extern crate htmlescape;
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

// `slog` must precede `log` in declarations here, because we want to simultaneously:
// * use the standard `log` macros (at least for a while)
// * be able to initialize the slog logger using slog macros like o!()
#[macro_use] extern crate slog;
#[macro_use] extern crate log;
// TODO: when migrating to slog completely, `log` can be removed and order restored

#[cfg(test)] extern crate tempfile;


#[macro_use]
mod util;

mod args;
mod commands;
mod ext;
mod gist;
mod hosts;
mod logging;

#[cfg(test)]
mod testing;


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
use util::exitcode::{self, ExitCode};


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
        env::home_dir().unwrap_or_else(env::temp_dir).join(&format!(".{}", *NAME));
    // TODO: use the app_dirs crate to get this in a more portable way

    /// Directory where gist sources are stored.
    ///
    /// Subdirectories are structured in a host-specific way,
    /// usually in a flat structure of gist ID-named directories
    /// e.g. `~/.gisht/gists/gh/4242` (a directory) for some `gh:Octocat/foo`.
    static ref GISTS_DIR: PathBuf = APP_DIR.join("gists");

    /// Directory where (symbolic) links to gist "binaries" are stored.
    ///
    /// Subdirectories are structured by host & the remaining part of gist URI,
    /// e.g. `~/.gisht/bin/gh/Octocat/foo` (a symlink) for `gh:Octocat/foo`.
    static ref BIN_DIR: PathBuf = APP_DIR.join("bin");
}


fn main() {
    let opts = args::parse().unwrap_or_else(|e| {
        print_args_error(e).unwrap();
        exit(exitcode::EX_USAGE);
    });

    logging::init(opts.verbosity).unwrap();
    trace!("{} {}{}", *NAME,
        VERSION.map(|v| format!("v{}", v)).unwrap_or_else(|| "<UNKNOWN VERSION>".into()),
        REVISION.map(|r| format!(" ({})", r)).unwrap_or_else(|| "".into()));

    ensure_app_dir(&opts).unwrap_or_else(|e| exit(e));

    let gist = decode_gist(&opts).unwrap_or_else(|e| exit(e));
    let exitcode = match opts.command {
        Command::Run => run_gist(&gist, opts.gist_args.as_ref().unwrap()),
        Command::Which => print_binary_path(&gist),
        Command::Print => print_gist(&gist),
        Command::Open => open_gist(&gist),
        Command::Info => show_gist_info(&gist),
    };
    exit(exitcode)
}

/// Print an error that may occur while parsing arguments.
fn print_args_error(e: ArgsError) -> io::Result<()> {
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
    }
}


/// Ensure that application directory exists.
/// If it needs to be created, this will be treated as application's first run.
fn ensure_app_dir(opts: &Options) -> Result<(), ExitCode> {
    if APP_DIR.exists() {
        trace!("Application directory ({}) already exists, skipping creation.",
            APP_DIR.display());
        return Ok(());
    }

    // If the first run is interactive, display a warning about executing untrusted code.
    if isatty::stderr_isatty() && !opts.quiet() {
        trace!("Displaying warning about executing untrusted code...");
        let should_continue = display_warning().unwrap();
        if !should_continue {
            debug!("Warning not acknowledged -- exiting.");
            return Err(exitcode::EX_UNKNOWN);
        }
        trace!("Warning acknowledged.");
    } else {
        trace!("Quiet/non-interactive run, skipping untrusted code warning.");
    }

    trace!("Creating application directory ({})...", APP_DIR.display());
    if let Err(err) = fs::create_dir_all(&*APP_DIR) {
        error!("Failed to create application directory ({}): {}",
            APP_DIR.display(), err);
        return Err(exitcode::EX_OSFILE);
    }
    debug!("Application directory ({}) created successfully.", APP_DIR.display());
    Ok(())
}


/// Use command line arguments to obtain a Gist object.
/// This may include fetching a fresh gist from a host, or updating it.
/// If an error occurred, returns the corresponding exit code.
fn decode_gist(opts: &Options) -> Result<Gist, ExitCode> {
    let gist = match opts.gist {
        GistArg::Uri(ref uri) => {
            debug!("Gist {} specified as the argument", uri);
            Gist::from_uri(uri.clone())
        },
        GistArg::BrowserUrl(ref url) => {
            debug!("Gist URL `{}` specified as the argument", url);
            let url = url.as_str();
            let maybe_gist = try!(gist_from_url(url));
            let gist = try!(maybe_gist.ok_or_else(|| {
                error!("URL doesn't point to any gist service: {}", url);
                exitcode::EX_UNAVAILABLE
            }));
            gist
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
                return Err(exitcode::EX_IOERR);
            }
        },
        Some(Locality::Local) => {
            if !is_local {
                error!("Gist {} is not available locally -- exiting.", gist.uri);
                return Err(exitcode::EX_NOINPUT);
            }
        },
        Some(Locality::Remote) => {
            debug!("Forcing update of gist {}...", gist.uri);
            if let Err(err) = gist.uri.host().fetch_gist(&gist, FetchMode::Always) {
                error!("Failed to update gist {}: {}", gist.uri, err);
                return Err(exitcode::EX_IOERR);
            }
        },
    }

    Ok(gist)
}

/// Ask each of the known gist hosts if they can resolve this URL into a gist.
fn gist_from_url(url: &str) -> Result<Option<Gist>, ExitCode> {
    let mut gists = Vec::new();

    for (id, host) in &*hosts::HOSTS {
        if let Some(res) = host.resolve_url(url) {
            let gist = try!(res.map_err(|err| {
                error!("Error asking {} to resolve gist from URL `{}`: {}",
                    host.name(), url, err);
                exitcode::EX_IOERR
            }));
            trace!("URL `{}` identified as `{}` ({}) gist", url, id, host.name());
            gists.push(gist);
        }
    }

    // If more that one host matches, it's an inconsistency in host definitions.
    // Since we cannot determine with host "wins", we can only bail.
    if gists.len() > 1 {
        let hosts_csv = gists.into_iter().map(|gist| {
            let host = gist.uri.host();
            format!("{} ({})", host.name(), host.id())
        }).collect::<Vec<_>>().join(", ");
        error!("Multiple matching hosts for URL `{}`: {}", url, hosts_csv);
        return Err(exitcode::EX_CONFIG);
    }

    Ok(gists.pop())
}


/// Display warning about executing untrusted code and ask the user to continue.
/// Returns whether the user decided to continue.
fn display_warning() -> io::Result<bool> {
    try!(writeln!(&mut io::stderr(), "{}", format_warning_message()));

    try!(write!(&mut io::stderr(), "{}", format_warning_ack_prompt()));
    let mut answer = String::with_capacity(YES.len());
    try!(io::stdin().read_line(&mut answer));

    Ok(answer.trim().to_lowercase() == YES)
}

/// Return the formatted warning message, incl. coloring if the terminal supports it.
fn format_warning_message() -> String {
    const PREFIX: &'static str = "WARNING";
    const WARNING: &'static [&'static str] = &[
        "${app} is used to download & run code from remote sources.",
        "",
        "Never run gists that you haven't authored, and/or do not trust.",
        "Doing so is dangerous, and may expose your system to security risks!",
        "",
        "(If you continue, this warning won't be shown again).",
        "",
    ];
    let prefix_style =
        if cfg!(unix) { Colour::Yellow.bold() } else { Style::default() };
    format!("{}: {}", prefix_style.paint(PREFIX),
        WARNING.join(util::LINESEP).replace("${app}", *NAME))
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
