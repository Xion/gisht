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
             extern crate isatty;
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
use std::fs;
use std::io::{self, Read, Write};
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

    // If this is a first run and it's interactive,
    // display a warning about executing untrusted code.
    if !APP_DIR.exists() {
        if isatty::stdout_isatty() && !opts.quiet() {
            display_warning();
            if let Err(err) = fs::create_dir_all(&*APP_DIR) {
                error!("Failed to create application directory ({}): {}",
                    APP_DIR.display(), err);
                exit(1);
            }
        }
    }

    if let Some(cmd) = opts.command {
        let gist_uri = opts.gist.unwrap();
        debug!("Gist {} specified as the argument", gist_uri);

        let mut gist = Gist::from_uri(gist_uri.clone());
        if !gist.is_local() {
            // TODO: this part is evidently GitHub-specific;
            // GitHub::download_gist should probably try to resolve gist URI
            // to obtain ID based on target binary of gist's symlink
            // (and if the gist isn't local then do the gist iteration thingie)
            let host = gist_uri.host();
            let gists = host.gists(&gist_uri.owner);
            gist = match gists.into_iter().find(|g| gist_uri == g.uri) {
                Some(gist) => gist,
                _ => { error!("Gist {} not found", gist_uri); exit(2); },
            };
            if let Err(err) = host.download_gist(&gist) {
                error!("Failed to download gist {}: {}", gist.uri, err);
                exit(2);
            }
        }

        match cmd {
            args::Command::Run => run_gist(&gist, opts.gist_args.as_ref().unwrap()),
            args::Command::Which => print_binary_path(&gist),
            args::Command::Print => print_gist(&gist),
            _ => unimplemented!(),
        }
    } else {
        debug!("No gist command specified -- exiting.");
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


/// Run the specified gist.
/// Regardless whether or not it succceeds, this function does not return.
fn run_gist(gist: &Gist, args: &[String]) -> ! {
    let uri = gist.uri.clone();

    let mut command = Command::new(gist.binary_path());
    command.args(args);

    // On Unix, we can replace the app's process completely with gist's executable
    // but on Windows, we have to run it as a child process and wait for it.
    if cfg!(unix) {
        use std::os::unix::process::CommandExt;

        // This calls execvp() and doesn't return unless an error occurred.
        // The process isn't really usable afterwards, so we just panic.
        let error = command.exec();
        panic!("Failed to execute gist {}: {}", uri, error);
    } else {
        let mut run = command.spawn()
            .unwrap_or_else(|e| panic!("Failed to execute gist {}: {}", uri, e));

        // Propagate thes same exit code that the gist binary returned.
        let exit_status = run.wait()
            .unwrap_or_else(|e| panic!("Failed to obtain status code for gist {}: {}", uri, e));
        let exit_code = exit_status.code().unwrap_or(127);
        exit(exit_code);
    }
}

/// Output the gist's binary path.
fn print_binary_path(gist: &Gist) -> ! {
    trace!("Printing binary path of {:?}", gist);
    println!("{}", gist.binary_path().display());
    exit(0);
}

/// Print the source of the gist's binary.
fn print_gist(gist: &Gist) -> ! {
    trace!("Printing source code of {:?}", gist);
    let binary = fs::File::open(gist.binary_path())
        .unwrap_or_else(|e| panic!("Failed to open the binary of gist {}: {}", gist.uri, e));
    for byte in binary.bytes() {
        let byte = byte
            .unwrap_or_else(|e| panic!("Falled to read to the binary of gist {}: {}", gist.uri, e));
        io::stdout().write_all(&[byte]).unwrap();
    }
    exit(0);
}
