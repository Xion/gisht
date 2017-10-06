//! Module implementing guessing of interpreters based on things like hashbang.

use std::borrow::Cow;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use itertools::Itertools;

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
    let path_prefix = hashbang_path.to_owned() + " ";
    let mut interpreters = vec![];
    for interp in COMMON_INTERPRETERS.values() {
        let starts_with_program = program.map(|p| interp.binary() == p).unwrap_or(false);
        if interp.command_line().starts_with(&path_prefix) || starts_with_program {
            interpreters.push(interp.clone());
        }
    }
    match interpreters.len() {
        0 => {
            debug!("Unrecognized gist binary hashbang: #!{}", hashbang_path);
            None
        },
        1 => {
            let result = interpreters.into_iter().next().unwrap();
            debug!("Guessed the interpreter for hashbang #!{} as `{}`",
                hashbang_path, result);
            Some(result)
        },
        _ => {
            debug!("Ambiguous hashbang #!{} resolves to multiple possible interpreters:\n{}",
                hashbang_path, interpreters.into_iter().format("\n"));
            None
        },
    }
}


#[cfg(test)]
mod tests {
    use std::io::Write;
    use tempfile::NamedTempFile;
    use super::*;

    #[test]
    fn interpreter_for_filename() {
        let guess = |f| guess_interpreter_for_filename(f)
            .map(|i| i.command_line().to_owned());
        assert_eq!(None, guess("/foo/bar"));  // no extension
        assert_eq!(None, guess("/foo.lolwtf"));  // unknown extension
        assert_eq!(Some("python ${script} - ${args}".into()), guess("/foo.py"));
    }

    #[test]
    fn interpreter_for_language() {
        let guess = |l| guess_interpreter_for_language(l)
            .map(|i| i.command_line().to_owned());
        assert_eq!(None, guess(""));
        assert_eq!(None, guess("GNU/Ruby#.NET"));
        assert_eq!(Some("python ${script} - ${args}".into()), guess("Python"));
        // File extension also works as a "language".
        assert_eq!(Some("python ${script} - ${args}".into()), guess("py"));
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
        assert_eq!(Some("python ${script} - ${args}".into()), guess("#!python"));
        assert_eq!(Some("python ${script} - ${args}".into()), guess("#!/usr/bin/python"));
    }
}
