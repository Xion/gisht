//!
//! gisht -- Gists in the shell
//!

             extern crate clap;
             extern crate conv;
#[macro_use] extern crate custom_derive;
#[macro_use] extern crate enum_derive;
             extern crate fern;
             extern crate git2;
             extern crate hyper;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
#[macro_use] extern crate maplit;
             extern crate regex;
             extern crate rustc_serialize;
             extern crate url;


mod args;
mod ext;
mod gist;
mod github;
mod logging;
mod util;


use std::env;
use std::path::PathBuf;
use std::process::{Command, exit};

use gist::Gist;


lazy_static!{
    /// User-Agent header that the program uses for all outgoing HTTP requests.
    static ref USER_AGENT: String =
        if let Some(version) = option_env!("CARGO_PKG_VERSION") {
            format!("gisht/{}", version)
        } else {
            "gisht".to_owned()
        };
}

lazy_static!{
    /// Main application's directory.
    static ref APP_DIR: PathBuf = {
        let mut dir = env::home_dir().unwrap_or_else(|| env::temp_dir());
        dir.push(".gisht");
        dir
    };

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
    let opts = args::parse();
    logging::init(opts.verbose()).unwrap();

    // TODO: ensure application direcotry exists

    if let Some(cmd) = opts.command {
        let gist_uri = opts.gist.unwrap();
        debug!("Gist {} specified as the argument", gist_uri);

        let host = gist_uri.host();
        let gists = host.gists(&gist_uri.owner);
        let gist = match gists.iter().find(|g| gist_uri == g.uri) {
            Some(gist) => gist,
            _ => { error!("Gist {} not found", gist_uri); exit(2); },
        };

        if !gist.is_local() {
            if let Err(err) = host.download_gist(&gist) {
                error!("Failed to download gist {}: {}", gist.uri, err);
                exit(2);
            }
        }

        match cmd {
            args::Command::Run => run_gist(gist),
            _ => unimplemented!(),
        }
    }
}


/// Run the specified gist.
// TODO: accept arguments
fn run_gist(gist: &Gist) {
    let uri = gist.uri.clone();
    let mut run = Command::new(gist.binary_path()).spawn()
        .unwrap_or_else(|e| panic!("Failed to execute gist {}: {}", uri, e));

    // Propagate thes same exit code that the gist binary returned.
    let exit_status = run.wait()
        .unwrap_or_else(|e| panic!("Failed to obtain status code for gist {}: {}", uri, e));
    let exit_code = exit_status.code().unwrap_or(127);
    exit(exit_code);
}
