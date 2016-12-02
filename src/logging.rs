//! Module implementing logging for the application.

use std::env;

use log::SetLoggerError;
use slog::{self, DrainExt, FilterLevel};
use slog_envlogger::LogBuilder;
use slog_stdlog;
use slog_term;


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
    // TODO: use slog_stream crate to better control log formatting;
    // example: https://github.com/slog-rs/misc/blob/master/examples/global_file_logger.rs
    let stderr = slog_term::streamer().sync().stderr()
        .use_custom_timestamp(move |io| write!(io, ""));  // No log timestamps.

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
    let mut builder = LogBuilder::new(stderr.build());
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


#[cfg(test)]
mod tests {
    use slog::FilterLevel;
    use super::{NEGATIVE_VERBOSITY_LEVELS, POSITIVE_VERBOSITY_LEVELS};

    #[test]
    fn verbosity_levels() {
        assert_eq!(NEGATIVE_VERBOSITY_LEVELS[0], POSITIVE_VERBOSITY_LEVELS[0]);
        assert!(NEGATIVE_VERBOSITY_LEVELS.contains(&FilterLevel::Off),
            "Verbosity levels don't allow to turn logging off completely");
    }
}
