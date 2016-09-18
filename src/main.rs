//!
//! gisht -- Gists in the shell
//!

             extern crate clap;
             extern crate conv;
#[macro_use] extern crate custom_derive;
#[macro_use] extern crate enum_derive;
             extern crate env_logger;
#[macro_use] extern crate error_derive;
             extern crate git2;
             extern crate hyper;
             extern crate isatty;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
#[macro_use] extern crate maplit;
             extern crate regex;
             extern crate rustc_serialize;
             extern crate shlex;
             extern crate url;


mod args;
mod commands;
mod ext;
mod gist;
mod hosts;
mod logging;
mod util;


use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::exit;

use args::{Command, Locality, Options};
use commands::{run_gist, print_binary_path, print_gist};
use gist::Gist;
use util::exitcode;


lazy_static! {
    /// User-Agent header that the program uses for all outgoing HTTP requests.
    static ref USER_AGENT: String =
        if let Some(version) = option_env!("CARGO_PKG_VERSION") {
            format!("gisht/{}", version)
        } else {
            "gisht".to_owned()
        };
}

lazy_static! {
    /// Main application's directory.
    static ref APP_DIR: PathBuf =
        env::home_dir().unwrap_or_else(|| env::temp_dir()).join(".gisht");

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
        writeln!(&mut io::stderr(), "Failed to parse argv; {}", e).unwrap();
        exit(exitcode::EX_USAGE);
    });
    logging::init(opts.verbosity).unwrap();

    ensure_app_dir(&opts);

    let gist = decode_gist(&opts);
    match opts.command {
        Command::Run => run_gist(&gist, opts.gist_args.as_ref().unwrap()),
        Command::Which => print_binary_path(&gist),
        Command::Print => print_gist(&gist),
        _ => unimplemented!(),
    }
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
    if isatty::stdout_isatty() && !opts.quiet() {
        trace!("Displaying warning about executing untrusted code...");
        let should_continue = display_warning();
        if !should_continue {
            debug!("Warning not acknowledged -- exiting.");
            exit(2);
        }
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
    let uri = opts.gist_uri.clone();
    debug!("Gist {} specified as the argument", uri);

    let gist = Gist::from_uri(uri);
    if gist.is_local() {
        trace!("Gist {} found among already downloaded gists", gist.uri);
        if opts.locality == Some(Locality::Remote) {
            // --fetch on exisiting gists is NYI
            unimplemented!();
            // TODO: perform a Git pull on exisiting gist repo; Host::fetch_gist can do that
        }
    } else {
        if opts.locality == Some(Locality::Local) {
            error!("Gist {} is not available locally -- exiting.", gist.uri);
            exit(exitcode::EX_NOINPUT);
        }
        if let Err(err) = gist.uri.host().fetch_gist(&gist) {
            error!("Failed to download gist {}: {}", gist.uri, err);
            exit(exitcode::EX_IOERR);
        }
    }

    gist
}


/// Display warning about executing untrusted code and ask the user to continue.
/// Returns whether the user decided to continue.
fn display_warning() -> bool {
    const WARNING: &'static [&'static str] = &[
        "WARNING: gisht is used to download & run code from a remote source.",
        "",
        "Never run gists that you haven't authored, and/or do not trust.",
        "Doing so is dangerous, and may expose your system to security risks!",
        "",
        "(If you continue, this warning won't be shown again).",
        "",
    ];
    writeln!(&mut io::stderr(), "{}", WARNING.join(util::LINESEP)).unwrap();

    write!(&mut io::stderr(), "{}", "Do you want to continue? [y/N]: ").unwrap();
    let mut answer = String::with_capacity(1);
    io::stdin().read_line(&mut answer).unwrap();

    answer.trim().to_lowercase() == "y"
}
