//! Tests for the `run` command.

#[cfg(unix)]
mod unix {
    use std::io::Write;
    use shlex;
    use tempfile::NamedTempFile;
    use regex::Regex;
    use super::super::{COMMON_INTERPRETERS, LANGUAGE_MAP,
                       guess_interpreter_for_filename,
                       guess_interpreter_for_language,
                       guess_interpreter_for_hashbang};

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

    #[test]
    fn interpreter_for_filename() {
        let guess = guess_interpreter_for_filename;
        assert_eq!(None, guess("/foo/bar"));  // no extension
        assert_eq!(None, guess("/foo.lolwtf"));  // unknown extension
        assert_eq!(Some("python ${script} - ${args}"), guess("/foo.py"));
    }

    #[test]
    fn interpreter_for_language() {
        let guess = guess_interpreter_for_language;
        assert_eq!(None, guess(""));
        assert_eq!(None, guess("GNU/Ruby#.NET"));
        assert_eq!(Some("python ${script} - ${args}"), guess("Python"));
        // File extension also works as a "language".
        assert_eq!(Some("python ${script} - ${args}"), guess("py"));
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
        };
        assert_eq!(None, guess(""));
        assert_eq!(None, guess("/not/a/hashbang/but/python"));
        assert_eq!(Some("python ${script} - ${args}"), guess("#!python"));
        assert_eq!(Some("python ${script} - ${args}"), guess("#!/usr/bin/python"));
    }
}
