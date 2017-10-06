//! Module implementing the actual running of gist "binaries" (scripts).
//!
//! This includes guessing of the correct interpreter using
//! available information (hashbang, gist metadata, etc.).

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::process::Command;

use shlex;
use exitcode::{self, ExitCode};

use gist::Gist;


/// Run the specified gist.
///
/// If this function succeeds, it may not return (because the process will be
/// completely replaced by the gist binary).
///
/// Otherwise, an exit code is returned.
pub fn run_gist(gist: &Gist, args: &[String]) -> ExitCode {
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
        const ERR_NO_SUCH_FILE: i32 = 2;  // For when hashbang is present but wrong.
        const ERR_EXEC_FORMAT: i32 = 8;  // For when hashbang is absent.

        // This calls execvp() and doesn't return unless an error occurred.
        let mut error = command.exec();
        debug!("Executing {:?} failed: {}", command, error);

        // If the problem was with hashbang (or lack thereof),
        // we'll try to infer a common interpreter based on gist's metadata
        // and feed it to its interpreter manually.
        if [ERR_NO_SUCH_FILE, ERR_EXEC_FORMAT].iter().any(|&e| error.raw_os_error() == Some(e)) {
            trace!("Invalid executable format of {}", binary.display());
            warn!("Couldn't run gist {} directly; it may not have a proper hashbang.", uri);
            if let Some(interpreter) = guess_interpreter(gist) {
                error = interpreted_run(interpreter, &binary, args);
            } else {
                error!("Failed to guess an interpreter for gist {}", uri);
            }
        }
        error!("Failed to execute gist {}: {}", uri, error);
        exitcode::UNAVAILABLE
    } else {
        let mut run = match command.spawn() {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to execute gist {}: {}", uri, e);
                return exitcode::TEMPFAIL;
            }
        };

        // Propagate the same exit code that the gist binary returned.
        let exit_status = match run.wait() {
            Ok(es) => es,
            Err(e) => {
                error!("Failed to obtain status code for gist {}: {}", uri, e);
                return exitcode::TEMPFAIL;
            },
        };
        exit_status.code().unwrap_or(exitcode::UNAVAILABLE)
    }
}

/// Type of an interpreter command line.
type Interpreter = &'static str;

/// Guess an interpreter for given gist, using a variety of factors.
/// Returns the "format string" for the interpreter's command string.
#[cfg(unix)]
fn guess_interpreter(gist: &Gist) -> Option<Interpreter> {
    let binary_path = gist.binary_path();
    guess_interpreter_for_filename(&binary_path)
        .or_else(|| gist.main_language().and_then(guess_interpreter_for_language))
        .or_else(|| guess_interpreter_for_hashbang(&binary_path))
}

/// Guess an interpreter for given binary file based on its file extension.
/// Returns the "format string" for the interpreter's command string.
#[cfg(unix)]
fn guess_interpreter_for_filename<P: AsRef<Path>>(binary_path: P) -> Option<Interpreter> {
    let binary_path = binary_path.as_ref();
    trace!("Trying to guess an interpreter for {}", binary_path.display());

    let extension = match binary_path.extension() {
        Some(ext) => ext,
        None => {
            warn!("Can't deduce interpreter w/o a binary file extension (got {})",
                binary_path.display());
            return None;
        },
    };

    let extension = try_opt!(extension.to_str());
    let interpreter = try_opt!(COMMON_INTERPRETERS.get(&extension));
    debug!("Guessed the interpreter for {} as `{}`",
        binary_path.display(), interpreter.split_whitespace().next().unwrap());
    Some(interpreter)
}

/// Guess an interpreter for a file written in given language.
/// Returns the "format string" for the interpreter's command string.
#[cfg(unix)]
fn guess_interpreter_for_language(language: &str) -> Option<Interpreter> {
    trace!("Trying to guess an interpreter for {} language", language);

    // Make the language name lowercase.
    let lang: Cow<str> = if language.chars().all(char::is_lowercase) {
        Cow::Borrowed(language)
    } else {
        Cow::Owned(language.to_lowercase())
    };

    // Determine the file extension for this language.
    // In some cases, the "language" may actually be an extension already,
    // so check for that case, too.
    let extension: Cow<str> =
        if LANGUAGE_MAP.values().any(|&ext| ext == &*lang) {
            lang
        } else {
            match LANGUAGE_MAP.get(&*lang) {
                Some(ext) => Cow::Borrowed(ext),
                None => {
                    debug!("Unsupported gist language: {}", language);
                    return None;
                },
            }
        };

    let interpreter = try_opt!(COMMON_INTERPRETERS.get(&*extension));
    debug!("Guessed the interpreter for {} language as `{}`",
        language, interpreter.split_whitespace().next().unwrap());
    Some(interpreter)
}

/// Guess an interpreter for a file based on its hashbang.
/// Returns the "format string" for the interpreter's command string.
#[cfg(unix)]
fn guess_interpreter_for_hashbang<P: AsRef<Path>>(binary_path: P) -> Option<Interpreter> {
    let binary_path = binary_path.as_ref();
    trace!("Trying to guess an interpreter for a possible hashbang in {}",
        binary_path.display());

    // Get the path mentioned in the hashbang, if any.
    let file = try_opt!(fs::File::open(binary_path).ok());
    let reader = BufReader::new(file);
    let first_line = try_opt!(reader.lines().next().and_then(|l| l.ok()));
    if !first_line.starts_with("#!") {
        debug!("Gist binary {} doesn't start with a hashbang", binary_path.display());
        return None;
    }
    let hashbang_path = &first_line[2..];

    // XXX: don't assume the entire hashbang is a path; POSIX allows a single argument
    // to appear after it, too
    // TODO: treat `#!/usr/bin/env foo` hashbangs specially:
    // `foo` should be the program there

    // Check if a single known interpreter path starts with the program name,
    // or the entire hashbang path.
    let program = Path::new(hashbang_path).file_name().and_then(|n| n.to_str());
    let program_prefix = program.map(|p| p.to_owned() + " ");
    let path_prefix = hashbang_path.to_owned() + " ";
    let mut interpreters = vec![];
    for &cmdline in COMMON_INTERPRETERS.values() {
        let starts_with_program = program_prefix.as_ref()
            .map(|p| cmdline.starts_with(p)).unwrap_or(false);
        if cmdline.starts_with(&path_prefix) || starts_with_program {
            interpreters.push(cmdline);
        }
    }
    match interpreters.len() {
        0 => {
            debug!("Unrecognized gist binary hashbang: #!{}", hashbang_path);
            None
        },
        1 => {
            let result = interpreters[0];
            debug!("Guessed the interpreter for hashbang #!{} as `{}`",
                hashbang_path, result);
            Some(result)
        },
        _ => {
            debug!("Ambiguous hashbang #!{} resolves to multiple possible interpreters:\n{}",
                hashbang_path, interpreters.join("\n"));
            None
        },
    }
}

/// Execute a script using given interpreter.
/// The interpreter must be a "format string" containing placeholders
/// for script path and arguments.
#[cfg(unix)]
fn interpreted_run<P: AsRef<Path>>(interpreter: Interpreter,
                                   script: P, args: &[String]) -> io::Error {
    use std::os::unix::process::CommandExt;

    // Format the interpreter-specific command line.
    let script = script.as_ref();
    let args = args.iter().map(|a| shlex::quote(a)).collect::<Vec<_>>().join(" ");
    let cmd = interpreter.to_owned()
        .replace("${script}", &script.to_string_lossy())
        .replace("${args}", &args);

    // Split the final interpreter-invoking command into "argv"
    // that can be executed via Command/execvp().
    trace!("$ {}", cmd);
    let cmd_argv = shlex::split(&cmd).unwrap();
    let mut command = Command::new(&cmd_argv[0]);
    command.args(&cmd_argv[1..]);

    // If everything goes well, this will not return.
    let error = command.exec();
    debug!("Interpreted run of {} failed: {}", script.display(), error);
    error
}

#[cfg(unix)]
lazy_static! {
    /// Mapping of language names (lowercase) to their file extensions.
    /// Note that the extension doesn't have to occur in COMMON_INTERPRETERS map.
    static ref LANGUAGE_MAP: HashMap<&'static str, &'static str> = hashmap!{
        "bash" => "sh",
        "clojure" => "clj",
        "go" => "go",
        "golang" => "go",
        "haskell" => "hs",
        "javascript" => "js",
        "node" => "js",
        "nodejs" => "js",
        "perl" => "pl",
        "python" => "py",
        "ruby" => "rb",
        "rust" => "rs",
        "shell" => "sh",
    };

    /// Mapping of common interpreters from file extensions they can handle.
    ///
    /// Interpreters are defined as shell commands with placeholders
    /// for gist script name and its arguments.
    static ref COMMON_INTERPRETERS: HashMap<&'static str, Interpreter> = hashmap!{
        "hs" => "runhaskell ${script} ${args}",
        "js" => "node -e ${script} ${args}",
        "pl" => "perl -- ${script} ${args}",
        "py" => "python ${script} - ${args}",
        "rb" => "irb -- ${script} ${args}",
        "sh" => "sh -- ${script} ${args}",
    };
}


#[cfg(test)]
mod tests;
