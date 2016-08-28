//!
//! gisht -- Gists in the shell
//!

             extern crate clap;
             extern crate conv;
#[macro_use] extern crate custom_derive;
#[macro_use] extern crate enum_derive;
             extern crate fern;
             extern crate hyper;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
#[macro_use] extern crate maplit;
             extern crate regex;
             extern crate rustc_serialize;
             extern crate url;


mod args;
mod gist;
mod github;
mod logging;
mod util;


use std::env;
use std::path::PathBuf;


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

    // TODO: replace with actual functionality
    use gist::Host;
    let gh = github::GitHub::new();
    for gist_uri in gh.gists("Xion") {
        println!("{}", gist_uri);
    }
}

