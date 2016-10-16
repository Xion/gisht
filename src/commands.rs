//! Module implementing various commands that can be performed on gists.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command, exit};

use shlex;
use webbrowser;

use gist::Gist;
use util::exitcode;


/// Run the specified gist.
/// Regardless whether or not it succceeds, this function does not return.
pub fn run_gist(gist: &Gist, args: &[String]) -> ! {
    let uri = gist.uri.clone();
    let binary = gist.binary_path();
    debug!("Running gist {} ({})...", uri, binary.display());

    let mut command = Command::new(&binary);
    command.args(args);

    trace!("About to execute {:?}", command);

    // On Unix, we can replace the app's process completely with gist's executable
    // but on Windows, we have to run it as a child process and wait for it.
    if cfg!(unix) {
        use std::os::unix::process::CommandExt;
        const ERR_EXEC_FORMAT: i32 = 8;

        // This calls execvp() and doesn't return unless an error occurred.
        let mut error = command.exec();
        debug!("Executing {:?} failed: {}", command, error);

        // If the problem was the executable format, it may be a lack of proper hashbang.
        // We'll try to infer a common interpreter based on gist's file extension
        // and feed it to its interpreter manually.
        if error.raw_os_error() == Some(ERR_EXEC_FORMAT) {
            trace!("Invalid executable format of {}", binary.display());
            warn!("Couldn't run gist {} directly; it may not have a proper hashbang.", uri);
            if let Some(interpreter) = guess_interpreter(&binary) {
                error = interpreted_run(interpreter, &binary, args);
            } else {
                error!("Failed to guess an interpereter for gist {}", uri);
            }
        }
        panic!("Failed to execute gist {}: {}", uri, error);
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

/// Guess an interpreter for given binary file based on its file extension.
/// Returns the "format string" for the interpreter's command string.
#[cfg(unix)]
fn guess_interpreter<P: AsRef<Path>>(binary_path: P) -> Option<&'static str> {
    let extension = match binary_path.as_ref().extension() {
        Some(ext) => ext,
        None => {
            error!("Can't deduce interpreter w/o a binary file extension (got {})",
                binary_path.as_ref().display());
            return None;
        },
    };

    extension.to_str()
        .and_then(|ext| COMMON_INTERPRETERS.get(&ext).map(|i| *i))
}

/// Execute a script using given interpreter.
/// The interpreter must be a "format string" containing placeholders
/// for script path and arguments.
#[cfg(unix)]
fn interpreted_run<P: AsRef<Path>>(interpreter: &str,
                                   script: P, args: &[String]) -> io::Error {
    use std::os::unix::process::CommandExt;

    // Format the interpreter-specific command line.
    let args = args.iter().map(|a| shlex::quote(a)).collect::<Vec<_>>().join(" ");
    let cmd = interpreter.to_owned()
        .replace("${script}", &script.as_ref().to_string_lossy())
        .replace("${args}", &args);

    // Split the final interpreter-invoking command into "argv"
    // that can be executed via Command/execvp().
    let cmd_argv = shlex::split(&cmd).unwrap();
    let mut command = Command::new(&cmd_argv[0]);
    command.args(&cmd_argv[1..]);

    // If everything goes well, this will not return.
    let error = command.exec();
    debug!("Interpreted run of {} failed: {}", script.as_ref().display(), error);
    error
}

#[cfg(unix)]
lazy_static! {
    /// Mapping of common interpreters from file extensions they can handle.
    ///
    /// Interpreters are defined as shell commands with placeholders
    /// for gist script name and its arguments.
    static ref COMMON_INTERPRETERS: HashMap<&'static str, &'static str> = hashmap!{
        "hs" => "runhaskell ${script} ${args}",
        "js" => "node -e ${script} ${args}",
        "pl" => "perl -- ${script} ${args}",
        "py" => "python ${script} - ${args}",
        "rb" => "irb -- ${script} ${args}",
        "sh" => "sh -- ${script} ${args}",
    };
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
    let mut binary = fs::File::open(gist.binary_path()).unwrap_or_else(|e| {
        panic!("Failed to open the binary of gist {}: {}", gist.uri, e)
    });

    const BUF_SIZE: usize = 256;
    let mut buf = [0; BUF_SIZE];
    loop {
        let c = binary.read(&mut buf).unwrap_or_else(|e| {
            panic!("Falled to read the binary of gist {}: {}", gist.uri, e)
        });
        io::stdout().write_all(&buf[0..c]).unwrap();
        if c < BUF_SIZE { break }
    }
    exit(exitcode::EX_OK);
}


/// Open the gist's HTML page in the default system browser.
pub fn open_gist(gist: &Gist) -> ! {
    let url = gist.uri.host().gist_url(gist).unwrap_or_else(|e| {
        panic!("Failed to determine the URL of gist {}: {}", gist.uri, e)
    });
    webbrowser::open(&url).unwrap_or_else(|e| {
        panic!("Failed to open the URL of gist {} ({}) in the browser: {}",
            gist.uri, url, e)
    });
    exit(exitcode::EX_OK)
}


/// Show summary information about the gist.
pub fn show_gist_info(gist: &Gist) -> ! {
    trace!("Obtaining information on {:?}", gist);
    let maybe_info = gist.uri.host().gist_info(gist).unwrap_or_else(|e| {
        panic!("Failed to obtain information about gist {}: {}", gist.uri, e);
    });
    match maybe_info {
        Some(info) => { print!("{}", info); exit(exitcode::EX_OK) },
        None => exit(exitcode::EX_UNAVAILABLE),
    };
}


#[cfg(test)]
mod tests {
    #[cfg(unix)]
    mod unix {
        use shlex;
        use super::super::COMMON_INTERPRETERS;

        #[test]
        fn interpreter_command_placeholders() {
            for cmd in COMMON_INTERPRETERS.values() {
                assert!(cmd.contains("${script}"),
                    "`{}` doesn't contain a script path placeholder", cmd);
                assert!(cmd.contains("${args}"),
                    "`{}` doesn't contain a script args placeholder", cmd);
            }
        }

        #[test]
        fn interpreter_command_syntax() {
            for cmd in COMMON_INTERPRETERS.values() {
                let final_cmd = cmd.to_owned()
                    .replace("${script}", "foo")
                    .replace("${args}", "bar \"baz thud\"");
                let cmd_argv = shlex::split(&final_cmd);

                assert!(cmd_argv.is_some(),
                    "Formatted `{}` doesn't parse as a shell command", cmd);
                assert!(cmd_argv.unwrap().len() >= 3,  // interpreter + script path + script args
                    "Formatted `{}` is way too short to be valid", cmd);
            }
        }
    }
}
