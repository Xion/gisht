//! Module implementing various commands that can be performed on gists.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, exit};

use shlex;
use webbrowser;

use gist::Gist;
use util::exitcode;


// TODO: make the functions here return the exitcode rather than
// calling std::process::exit() themselves for better testability


// Running gists.

/// Run the specified gist.
/// Regardless whether or not it succeeds, this function does not return.
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
        exit(1);
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

    // Check if a single known interpreter path starts with the program name,
    // or the entire hashbang path.
    let program = Path::new(hashbang_path).file_name().and_then(|n| n.to_str());
    let prefix = program.map(|p| p.to_owned() + " ");
    let mut interpreters = vec![];
    for &cmdline in COMMON_INTERPRETERS.values() {
        let starts_with_program = prefix.as_ref()
            .map(|p| cmdline.starts_with(p)).unwrap_or(false);
        if cmdline.starts_with(&hashbang_path) || starts_with_program {
            interpreters.push(cmdline);
        }
    }
    match interpreters.len() {
        0 => None,
        1 => {
            debug!("Guessed the interpreter for hashbang #!{} as `{}`",
                hashbang_path, interpreters[0]);
            Some(interpreters[0])
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
            panic!("Failed to read the binary of gist {}: {}", gist.uri, e)
        });
        if c > 0 {
            io::stdout().write_all(&buf[0..c]).unwrap();
        }
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
        Some(info) => {
            debug!("Successfully obtained information on {:?}", gist);
            print!("{}", info);
            exit(exitcode::EX_OK);
        },
        None => {
            warn!("No information available about gist {}", gist.uri);
            exit(exitcode::EX_UNAVAILABLE);
        },
    };
}


#[cfg(test)]
mod tests {
    #[cfg(unix)]
    mod unix {
        use shlex;
        use regex::Regex;
        use super::super::{COMMON_INTERPRETERS, LANGUAGE_MAP};

        lazy_static! {
            static ref LOWERCASE_RE: Regex = Regex::new("^[a-z]+$").unwrap();
        }

        #[test]
        fn language_names() {
            for lang in LANGUAGE_MAP.keys() {
                assert!(LOWERCASE_RE.is_match(lang),
                    "Language name `{}` doesn't match the expected form {}", lang, *LOWERCASE_RE);
            }
        }

        #[test]
        fn language_file_extensions() {
            for ext in LANGUAGE_MAP.values() {
                assert!(!ext.starts_with("."),
                    "`{}` file extension must not start with a dot", ext);
                assert!(LOWERCASE_RE.is_match(ext),
                    "`{}` extension doesn't match the expected form {}", ext, *LOWERCASE_RE);
            }
        }

        #[test]
        fn interpreter_file_extensions() {
            for ext in COMMON_INTERPRETERS.keys() {
                assert!(!ext.starts_with("."),
                    "`{}` extension must not start with a dot", ext);
                assert!(LOWERCASE_RE.is_match(ext),
                    "`{}` extension doesn't match the expected form {}", ext, *LOWERCASE_RE);
            }
        }

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
                    .replace("${args}", r#"bar "baz thud" qux"#);
                let cmd_argv = shlex::split(&final_cmd);

                assert!(cmd_argv.is_some(),
                    "Formatted `{}` doesn't parse as a shell command", cmd);
                assert!(cmd_argv.unwrap().len() >= 3,  // interpreter + script path + script args
                    "Formatted `{}` is way too short to be valid", cmd);
            }
        }
    }
}
