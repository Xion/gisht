//! Module defining the common language interpreters that the program recognizes.
//!
//! This is only supported on Unix systems.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use shlex;


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
        "hs" => "runhaskell ${script} ${args}".into(),
        "js" => "node -e ${script} ${args}".into(),
        "pl" => "perl -- ${script} ${args}".into(),
        "py" => "python ${script} - ${args}".into(),
        "rb" => "irb -- ${script} ${args}".into(),
        "sh" => "sh -- ${script} ${args}".into(),
    };
}
const SCRIPT_PH: &'static str = "${script}";
const ARGS_PH: &'static str = "${args}";


/// Type representing an interpreter that can run gist's binary.
#[derive(Clone, Debug)]
pub struct Interpreter {
    /// "Format string" for the interpeter's commandline.
    /// Includes ${script} and ${args} placeholders.
    cmdline: &'static str,
    /// Additional arguments that should precede gist arguments in ${args}.
    innate_args: Vec<String>,
}

impl Interpreter {
    #[inline]
    pub fn with_cmdline(cmdline: &'static str) -> Self {
        Self::new(cmdline, vec![])
    }

    #[inline]
    pub fn new(cmdline: &'static str, innate_args: Vec<String>) -> Self {
        Interpreter { cmdline, innate_args }
    }
}

impl From<&'static str> for Interpreter {
    fn from(input: &'static str) -> Self {
        Interpreter::with_cmdline(input)
    }
}

impl Interpreter {
    #[inline]
    pub fn binary(&self) -> &str {
        self.cmdline.split_whitespace().next().unwrap()
    }

    #[cfg(test)]
    #[inline]
    pub fn command_line(&self) -> &str {
        self.cmdline
    }

    pub fn build_invocation<P: AsRef<Path>>(&self, script: P, args: &[String]) -> String {
        let script = script.as_ref();
        let args = self.innate_args.iter().chain(args.iter())
            .map(|a| shlex::quote(a)).collect::<Vec<_>>().join(" ");
        self.cmdline.to_owned()
            .replace(SCRIPT_PH, &script.to_string_lossy())
            .replace(ARGS_PH, &args)
    }
}

impl fmt::Display for Interpreter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.build_invocation(SCRIPT_PH, &[ARGS_PH.to_owned()]))
    }
}


/// Execute a script using given interpreter.
///
/// The interpreter must be a "format string" containing placeholders
/// for script path and arguments.
pub fn interpreted_run<P: AsRef<Path>>(interpreter: Interpreter,
                                       script: P, args: &[String]) -> io::Error {
    let script = script.as_ref();
    let cmd = interpreter.build_invocation(script, args);

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
    use super::{ARGS_PH, COMMON_INTERPRETERS, LANGUAGE_MAP, SCRIPT_PH};

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
        for interp in COMMON_INTERPRETERS.values() {
            assert!(interp.command_line().contains(SCRIPT_PH),
                "`{}` doesn't contain a script path placeholder", interp);
            assert!(interp.command_line().contains(ARGS_PH),
                "`{}` doesn't contain a script args placeholder", interp);
        }
    }

    #[test]
    fn interpreter_command_syntax() {
        for interp in COMMON_INTERPRETERS.values() {
            let final_cmd = interp.command_line().to_owned()
                .replace(SCRIPT_PH, "foo")
                .replace(ARGS_PH, r#"bar "baz thud" qux"#);
            let cmd_argv = shlex::split(&final_cmd);

            assert!(cmd_argv.is_some(),
                "Formatted `{}` doesn't parse as a shell command", interp);
            assert!(cmd_argv.unwrap().len() >= 3,  // interpreter + script path + script args
                "Formatted `{}` is way too short to be valid", interp);
        }
    }
}
