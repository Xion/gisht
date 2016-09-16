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

    // If this is a first run and it's interactive,
    // display a warning about executing untrusted code.
    if !APP_DIR.exists() {
        if isatty::stdout_isatty() && !opts.quiet() {
            display_warning();
            if let Err(err) = fs::create_dir_all(&*APP_DIR) {
                error!("Failed to create application directory ({}): {}",
                    APP_DIR.display(), err);
                exit(exitcode::EX_OSFILE);
            }
        }
    }

    debug!("Gist {} specified as the argument", opts.gist);
    let gist = Gist::from_uri(opts.gist.clone());
    if !gist.is_local() {
        if let Err(err) = opts.gist.host().download_gist(&gist) {
            error!("Failed to download gist {}: {}", gist.uri, err);
            exit(exitcode::EX_IOERR);
        }
    }

    match opts.command {
        args::Command::Run => run_gist(&gist, opts.gist_args.as_ref().unwrap()),
        args::Command::Which => print_binary_path(&gist),
        args::Command::Print => print_gist(&gist),
        _ => unimplemented!(),
    }
}

/// Display warning about executing untrusted code and ask the user to continue.
/// If declined, the program will end.
fn display_warning() {
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

    if answer.trim().to_lowercase() != "y" {
        exit(2);
    }
}
