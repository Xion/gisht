//! Module implementing the actual running of gist "binaries" (scripts).
//!
//! This includes guessing of the correct interpreter using
//! available information (hashbang, gist metadata, etc.).

#[cfg(unix)] mod guess;
#[cfg(unix)] mod interpreters;


use std::path::Path;
use std::process::Command;

use exitcode::{self, ExitCode};

use gist::Gist;
use self::guess::guess_interpreter;
use self::interpreters::interpreted_run;


/// Run the specified gist.
///
/// If this function succeeds, it may not return (because the process will be
/// completely replaced by the gist binary).
///
/// Otherwise, an exit code is returned.
pub fn run_gist(gist: &Gist, args: &[String]) -> ExitCode {
    let binary = gist.binary_path();
    debug!("Running gist {} ({})...", gist.uri, binary.display());

    // On Unix, we can replace the app's process completely with gist's executable
    // but on Windows, we have to run it as a child process and wait for it.
    exec_gist(gist, &binary, args)
}


#[cfg(unix)]
fn exec_gist(gist: &Gist, binary: &Path, args: &[String]) -> ExitCode {
    use std::os::unix::process::CommandExt;

    const ERR_NO_SUCH_FILE: i32 = 2;  // For when hashbang is present but wrong.
    const ERR_EXEC_FORMAT: i32 = 8;  // For when hashbang is absent.

    let mut command = build_command(binary, args);

    // This calls execvp() and doesn't return unless an error occurred.
    let mut error = command.exec();
    debug!("Executing {:?} failed: {}", command, error);

    // If the problem was with hashbang (or lack thereof),
    // we'll try to infer a common interpreter based on gist's metadata
    // and feed it to its interpreter manually.
    if [ERR_NO_SUCH_FILE, ERR_EXEC_FORMAT].iter().any(|&e| error.raw_os_error() == Some(e)) {
        trace!("Invalid executable format of {}", binary.display());
        warn!("Couldn't run gist {} directly; it may not have a proper hashbang.", gist.uri);
        if let Some(interpreter) = guess_interpreter(gist) {
            error = interpreted_run(interpreter, &binary, args);
        } else {
            error!("Failed to guess an interpreter for gist {}", gist.uri);
        }
    }
    error!("Failed to execute gist {}: {}", gist.uri, error);
    exitcode::UNAVAILABLE
}

#[cfg(not(unix))]
fn exec_gist(gist: &Gist, binary: &Path, args: &[String]) -> ExitCode {
    let mut command = build_command(binary, args);

    let mut run = match command.spawn() {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to execute gist {} through its binary {}: {}",
                gist.uri, binary.display(), e);
            return exitcode::TEMPFAIL;
        }
    };

    // Propagate the same exit code that the gist binary returned.
    let exit_status = match run.wait() {
        Ok(es) => es,
        Err(e) => {
            error!("Failed to obtain status code for gist {}: {}", gist.uri, e);
            return exitcode::TEMPFAIL;
        },
    };
    exit_status.code().unwrap_or(exitcode::UNAVAILABLE)
}


#[inline]
fn build_command(binary: &Path, args: &[String]) -> Command {
    let mut command = Command::new(&binary);
    command.args(args);

    trace!("About to execute {:?}", command);
    command
}
