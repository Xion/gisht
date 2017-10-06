//! Module defining the common language interpreters that the program recognizes.
//!
//! This is only supported on Unix systems.

use std::collections::HashMap;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use shlex;


/// Type of an interpreter command line.
pub type Interpreter = &'static str;


lazy_static! {
    /// Mapping of language names (lowercase) to their file extensions.
    /// Note that the extension doesn't have to occur in COMMON_INTERPRETERS map.
    pub static ref LANGUAGE_MAP: HashMap<&'static str, &'static str> = hashmap!{
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
    pub static ref COMMON_INTERPRETERS: HashMap<&'static str, Interpreter> = hashmap!{
        "hs" => "runhaskell ${script} ${args}",
        "js" => "node -e ${script} ${args}",
        "pl" => "perl -- ${script} ${args}",
        "py" => "python ${script} - ${args}",
        "rb" => "irb -- ${script} ${args}",
        "sh" => "sh -- ${script} ${args}",
    };
}


/// Execute a script using given interpreter.
///
/// The interpreter must be a "format string" containing placeholders
/// for script path and arguments.
pub fn interpreted_run<P: AsRef<Path>>(interpreter: Interpreter,
                                       script: P, args: &[String]) -> io::Error {
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


#[cfg(test)]
mod tests {
    use regex::Regex;
    use shlex;
    use super::{COMMON_INTERPRETERS, LANGUAGE_MAP};

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
