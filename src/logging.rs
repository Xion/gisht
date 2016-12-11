//! Module implementing logging for the application.
//!
//! This includes setting up log filtering given a verbosity value,
//! as well as defining how the logs are being formatted to stderr.

use std::env;
use std::io;

use log::SetLoggerError;
use slog::{self, DrainExt, FilterLevel, Level};
use slog_envlogger::LogBuilder;
use slog_stdlog;
use slog_stream;
use time;


// Default logging level defined using the two enums used by slog.
// Both values must correspond to the same level. (This is checked by a test).
const DEFAULT_LEVEL: Level = Level::Info;
const DEFAULT_FILTER_LEVEL: FilterLevel = FilterLevel::Info;

// Arrays of log levels, indexed by verbosity.
const POSITIVE_VERBOSITY_LEVELS: &'static [FilterLevel] = &[
    DEFAULT_FILTER_LEVEL,
    FilterLevel::Debug,
    FilterLevel::Trace,
];
const NEGATIVE_VERBOSITY_LEVELS: &'static [FilterLevel] = &[
    DEFAULT_FILTER_LEVEL,
    FilterLevel::Warning,
    FilterLevel::Error,
    FilterLevel::Critical,
    FilterLevel::Off,
];


/// Initialize logging with given verbosity.
/// The verbosity value has the same meaning as in args::Options::verbosity.
pub fn init(verbosity: isize) -> Result<(), SetLoggerError> {
    let stderr = slog_stream::stream(io::stderr(), LogFormat);

    // Determine the log filtering level based on verbosity.
    // If the argument is excessive, log that but clamp to the highest/lowest log level.
    let mut verbosity = verbosity;
    let mut excessive = false;
    let level = if verbosity >= 0 {
        if verbosity >= POSITIVE_VERBOSITY_LEVELS.len() as isize {
            excessive = true;
            verbosity = POSITIVE_VERBOSITY_LEVELS.len() as isize - 1;
        }
        POSITIVE_VERBOSITY_LEVELS[verbosity as usize]
    } else {
        verbosity = -verbosity;
        if verbosity >= NEGATIVE_VERBOSITY_LEVELS.len() as isize {
            excessive = true;
            verbosity = NEGATIVE_VERBOSITY_LEVELS.len() as isize - 1;
        }
        NEGATIVE_VERBOSITY_LEVELS[verbosity as usize]
    };

    // Include universal logger options, like the level.
    let mut builder = LogBuilder::new(stderr);
    builder = builder.filter(None, level);

    // Make some of the libraries less chatty.
    builder = builder.filter(Some("hyper"), FilterLevel::Info);

    // Include any additional config from environmental variables.
    // This will override the options above if necessary,
    // so e.g. it is still possible to get full debug output from hyper.
    if let Ok(ref conf) = env::var("RUST_LOG") {
        builder = builder.parse(conf);
    }

    // Initialize the logger, possibly logging the excessive verbosity option.
    // TODO: migrate off of `log` macro to slog completely,
    // so that slog_scope is used to set up the application's logger
    // and slog_stdlog is only for the libraries like hyper that use `log` macros
    let env_logger_drain = builder.build();
    let logger = slog::Logger::root(env_logger_drain.fuse(), o!());
    try!(slog_stdlog::set_logger(logger));
    if excessive {
        warn!("-v/-q flag passed too many times, logging level {:?} assumed", level);
    }
    Ok(())
}


/// Token type that's only uses to tell slog-stream how to format our log entries.
struct LogFormat;

impl slog_stream::Format for LogFormat {
    /// Format a single log Record and write it to given output.
    fn format(&self, output: &mut io::Write,
              record: &slog::Record,
              _logger_kvp: &slog::OwnedKeyValueList) -> io::Result<()> {
        // Format the higher level (more fine-grained) messages with greater detail,
        // as they are only visible when user explicitly enables verbose logging.
        let msg = if record.level() > DEFAULT_LEVEL {
            let now = time::now();
            let logtime = now.rfc3339();  // E.g.: 2012-02-22T07:53:18-07:00
            format!("{}{} {}#{}] {}\n",
                format_log_level(record.level()), logtime,
                record.module(), record.line(),
                record.msg())
        } else {
            // TODO: colorize the output (especially the level part)
            // if output is a TTY (which can be passed via logger_kvp)
            format!("{}: {}\n", record.level().as_str(), record.msg())
        };

        try!(output.write_all(msg.as_bytes()));
        Ok(())
    }
}

/// Format the log level string.
fn format_log_level(level: Level) -> String {
    let level = level.as_str();
    let first_char = level.chars().next().unwrap();
    first_char.to_uppercase().collect()
}


#[cfg(test)]
mod tests {
    use slog::FilterLevel;
    use super::{DEFAULT_LEVEL, DEFAULT_FILTER_LEVEL,
                NEGATIVE_VERBOSITY_LEVELS, POSITIVE_VERBOSITY_LEVELS};

    /// Check that default logging level is defined consistently.
    #[test]
    fn default_level() {
        let level = DEFAULT_LEVEL.as_usize();
        let filter_level = DEFAULT_FILTER_LEVEL.as_usize();
        assert_eq!(level, filter_level,
            "Default logging level is defined inconsistently: Level::{:?} vs. FilterLevel::{:?}",
            DEFAULT_LEVEL, DEFAULT_FILTER_LEVEL);
    }

    #[test]
    fn verbosity_levels() {
        assert_eq!(NEGATIVE_VERBOSITY_LEVELS[0], POSITIVE_VERBOSITY_LEVELS[0]);
        assert!(NEGATIVE_VERBOSITY_LEVELS.contains(&FilterLevel::Off),
            "Verbosity levels don't allow to turn logging off completely");
    }
}
