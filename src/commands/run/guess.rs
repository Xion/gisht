//! Module implementing guessing of interpreters based on things like hashbang.

use std::borrow::Cow;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use itertools::Itertools;
use shlex;

use gist::Gist;
use super::interpreters::*;


/// Guess an interpreter for given gist, using a variety of factors.
/// Returns the "format string" for the interpreter's command string.
pub fn guess_interpreter(gist: &Gist) -> Option<Interpreter> {
    let binary_path = gist.binary_path();
    guess_interpreter_for_filename(&binary_path)
        .or_else(|| gist.main_language().and_then(guess_interpreter_for_language))
        .or_else(|| guess_interpreter_for_hashbang(&binary_path))
}


/// Guess an interpreter for given binary file based on its file extension.
/// Returns the "format string" for the interpreter's command string.
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
        binary_path.display(), interpreter.binary());
    Some(interpreter.clone())
}


/// Guess an interpreter for a file written in given language.
/// Returns the "format string" for the interpreter's command string.
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
        language, interpreter.binary());
    Some(interpreter.clone())
}


/// Guess an interpreter for a file based on its hashbang.
///
/// This is normally not necessary as the operating system should resolve
/// the hashbang before we even start guessing (i.e. regular executable run).
/// However, if the hashbang is not entirely correct but salvageable
/// -- e.g. it says `/usr/bin/python` but the system's Python is somewhere else --
/// we can try to repair it this way.
///
/// Returns the "format string" for the interpreter's command string.
fn guess_interpreter_for_hashbang<P: AsRef<Path>>(binary_path: P) -> Option<Interpreter> {
    let binary_path = binary_path.as_ref();
    trace!("Trying to guess an interpreter for a possible hashbang in {}",
        binary_path.display());

    // Extract the hashbang, if any.
    let file = try_opt!(fs::File::open(binary_path).ok());
    let reader = BufReader::new(file);
    let first_line = try_opt!(reader.lines().next().and_then(|l| l.ok()));
    if !first_line.starts_with("#!") {
        debug!("Gist binary {} doesn't start with a hashbang", binary_path.display());
        return None;
    }
    let hashbang = &first_line[2..];

    // POSIX permits a single argument to appear after the hashbang program,
    // (which is often used for /usr/bin/env). We'll actually be more lenient
    // and allow arbitrary number of arguments and interpret it accordingly.
    let mut parts = try_opt!(shlex::split(hashbang));
    if parts.is_empty() {
        debug!("Gist binary {} starts with an empty hashbang", binary_path.display());
        return None;
    }
    let mut program = parts.remove(0);
    let mut innate_args = parts;

    // Special case for when the program is `env` in which case the actual name
    // of the interpreter is the second argument (e.g. `#!/usr/bin/env`).
    if program == "/usr/bin/env" || program == "/bin/env" {  // TODO: also plain "env"?
        if innate_args.is_empty() {
            debug!("Gist binary {} has an incorrect #!{} hashbang w/o an argument",
                binary_path.display(), program);
            return None;
        }
        program = innate_args.remove(0);
    }

    // Check if a single known interpreter path starts with the program name.
    let program_name = try_opt!(Path::new(&program).file_name().and_then(|n| n.to_str()));
    let interpreters: Vec<_> = COMMON_INTERPRETERS.values()
        .filter(|i| i.binary() == program_name)
        .cloned().collect();
    match interpreters.len() {
        0 => {
            debug!("Unrecognized gist binary hashbang: #!{}", hashbang);
            None
        },
        1 => {
            let result = interpreters.into_iter().next().unwrap();
            debug!("Guessed the interpreter for hashbang #!{} as `{}`",
                hashbang, result);
            Some(result)
        },
        _ => {
            debug!("Ambiguous hashbang #!{} resolves to multiple possible interpreters:\n{}",
                hashbang, interpreters.into_iter().format_with("\n", |i, f| f(&format_args!("* {}", i))));
            None
        },
    }
}


#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;
    use super::*;

    const PYTHON: &'static str = "python ${script} - ${args}";

    #[test]
    fn interpreter_for_filename() {
        let guess = |f| guess_interpreter_for_filename(f)
            .map(|i| i.command_line().to_owned());
        assert_eq!(None, guess("/foo/bar"));  // no extension
        assert_eq!(None, guess("/foo.lolwtf"));  // unknown extension
        assert_eq!(Some(PYTHON.into()), guess("/foo.py"));
    }

    #[test]
    fn interpreter_for_language() {
        let guess = |l| guess_interpreter_for_language(l)
            .map(|i| i.command_line().to_owned());
        assert_eq!(None, guess(""));
        assert_eq!(None, guess("GNU/Ruby#.NET"));
        assert_eq!(Some(PYTHON.into()), guess("Python"));
        // File extension also works as a "language".
        assert_eq!(Some(PYTHON.into()), guess("py"));
    }

    #[test]
    fn interpreter_for_hashbang() {
        let guess = |hashbang: &str| {
            // Prepare a temp file with the first line being the hashbang.
            let mut tmpfile = NamedTempFile::new().unwrap();
            let line = hashbang.to_owned() + "\n";
            tmpfile.write_all(&line.into_bytes()).unwrap();
            // Guess the interpreter for its path.
            guess_interpreter_for_hashbang(tmpfile.path())
                .map(|i| i.command_line().to_owned())
        };

        assert_eq!(None, guess(""));
        assert_eq!(None, guess("/not/a/hashbang/but/python"));

        assert_eq!(Some(PYTHON.into()), guess("#!python"));
        assert_eq!(Some(PYTHON.into()), guess("#!/usr/bin/python"));
        assert_eq!(Some(PYTHON.into()), guess("#!/usr/bin/python -O"));

        assert_eq!(Some(PYTHON.into()), guess("#!/usr/bin/env python"));
        assert_eq!(Some(PYTHON.into()), guess("#!/bin/env python -O"));

        // TODO: verify Interpreter::innate_args
    }
}
