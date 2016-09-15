//! Module implementing various commands that can be performed on gists.

use std::fs;
use std::io::{self, Read, Write};
use std::process::{Command, exit};

use log;

use gist::Gist;
use util::exitcode;


/// Run the specified gist.
/// Regardless whether or not it succceeds, this function does not return.
pub fn run_gist(gist: &Gist, args: &[String]) -> ! {
    let uri = gist.uri.clone();
    debug!("Running gist {}...", uri);

    let mut command = Command::new(gist.binary_path());
    command.args(args);

    trace!("About to execute {:?}", command);
    log::shutdown_logger().unwrap();

    // On Unix, we can replace the app's process completely with gist's executable
    // but on Windows, we have to run it as a child process and wait for it.
    if cfg!(unix) {
        use std::os::unix::process::CommandExt;

        // This calls execvp() and doesn't return unless an error occurred.
        // The process isn't really usable afterwards, so we just panic.
        let error = command.exec();
        panic!("Failed to execute gist {}: {}", uri, error);
        // TODO: if the gist doesn't have a proper hashbang, try to deduce the proper interpreter
        // based on the file extension instead
    } else {
        let mut run = command.spawn()
            .unwrap_or_else(|e| panic!("Failed to execute gist {}: {}", uri, e));

        // Propagate the same exit code that the gist binary returned.
        let exit_status = run.wait().unwrap_or_else(|e| {
            panic!("Failed to obtain status code for gist {}: {}", uri, e)
        });
        let exit_code = exit_status.code().unwrap_or(exitcode::EX_TEMPFAIL);
        exit(exit_code);
    }
}


/// Output the gist's binary path.
pub fn print_binary_path(gist: &Gist) -> ! {
    trace!("Printing binary path of {:?}", gist);
    println!("{}", gist.binary_path().display());
    exit(exitcode::EX_OK);
}


/// Print the source of the gist's binary.
pub fn print_gist(gist: &Gist) -> ! {
    trace!("Printing source code of {:?}", gist);
    let binary = fs::File::open(gist.binary_path()).unwrap_or_else(|e| {
        panic!("Failed to open the binary of gist {}: {}", gist.uri, e)
    });
    for byte in binary.bytes() {
        let byte = byte.unwrap_or_else(|e| {
            panic!("Falled to read to the binary of gist {}: {}", gist.uri, e)
        });
        io::stdout().write_all(&[byte]).unwrap();
    }
    exit(exitcode::EX_OK);
}
