//!
//! gisht -- Gists in the shell
//!

             extern crate clap;
             extern crate conv;
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


lazy_static!{
    // User-Agent header that the program uses for all outgoing HTTP requests.
    static ref USER_AGENT: String =
        if let Some(version) = option_env!("CARGO_PKG_VERSION") {
            format!("gisht/{}", version)
        } else {
            "gisht".to_owned()
        };
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

