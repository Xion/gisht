//! Module implementing various commands that can be performed on gists.

use std::fs;
use std::io::{self, Read, Write};

use exitcode::{self, ExitCode};
use webbrowser;

use gist::Gist;


/// Output the gist's binary path.
pub fn print_binary_path(gist: &Gist) -> ExitCode {
    trace!("Printing binary path of {:?}", gist);
    println!("{}", gist.binary_path().display());
    exitcode::OK
}


/// Print the source of the gist's binary.
pub fn print_gist(gist: &Gist) -> ExitCode {
    trace!("Printing source code of {:?}", gist);
    let mut binary = match fs::File::open(gist.binary_path()) {
        Ok(file) => file,
        Err(e) => {
            error!("Failed to open the binary of gist {}: {}", gist.uri, e);
            return exitcode::IOERR;
        },
    };

    const BUF_SIZE: usize = 256;
    let mut buf = [0; BUF_SIZE];
    loop {
        let c = match binary.read(&mut buf) {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to read the binary of gist {}: {}", gist.uri, e);
                return exitcode::IOERR;
            },
        };
        if c > 0 {
            if let Err(e) = io::stdout().write_all(&buf[0..c]) {
                error!("Failed to write the gist {} to stdout: {}", gist.uri, e);
                return exitcode::IOERR;
            }
        }
        if c < BUF_SIZE { break }
    }
    exitcode::OK
}


/// Open the gist's HTML page in the default system browser.
pub fn open_gist(gist: &Gist) -> ExitCode {
    let url = match gist.uri.host().gist_url(gist) {
        Ok(url) => url,
        Err(e) => {
            error!("Failed to determine the URL of gist {}: {}", gist.uri, e);
            return exitcode::UNAVAILABLE;
        },
    };
    if let Err(e) = webbrowser::open(&url) {
        error!("Failed to open the URL of gist {} ({}) in the browser: {}",
            gist.uri, url, e);
        return exitcode::UNAVAILABLE;
    };
    exitcode::OK
}


/// Show summary information about the gist.
pub fn show_gist_info(gist: &Gist) -> ExitCode {
    trace!("Obtaining information on {:?}", gist);
    match gist.uri.host().gist_info(gist) {
        Ok(Some(info)) => {
            debug!("Successfully obtained {} piece(s) of information on {:?}",
                info.len(), gist);
            print!("{}", info);
            exitcode::OK
        },
        Ok(None) => {
            warn!("No information available about {:?}", gist);
            exitcode::UNAVAILABLE
        },
        Err(e) => {
            error!("Failed to obtain information about {:?}: {}", gist, e);
            exitcode::UNAVAILABLE
        },
    }
}

