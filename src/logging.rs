//! Module implementing logging for the application.

use std::env;

use env_logger::LogBuilder;
use log::{LogLevel, LogLevelFilter, LogRecord, SetLoggerError};


// Arrays of log level filters, indexed by verbosity.
const POSITIVE_VERBOSITY_LEVELS: &'static [LogLevelFilter] = &[
    LogLevelFilter::Info,
    LogLevelFilter::Debug,
    LogLevelFilter::Trace,
];
const NEGATIVE_VERBOSITY_LEVELS: &'static [LogLevelFilter] = &[
    LogLevelFilter::Info,
    LogLevelFilter::Warn,
    LogLevelFilter::Error,
    LogLevelFilter::Off,
];


/// Initialize logging with given verbosity.
/// The verbosity value has the same meaning as in args::Options::verbosity.
pub fn init(verbosity: isize) -> Result<(), SetLoggerError> {
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

    // Include universal logger options, like the formatting function.
    let mut builder = LogBuilder::new();
    builder.format(format_log_record).filter(None, level);

    // Make some of the libraries less chatty.
    builder.filter(Some("hyper"), LogLevelFilter::Info);

    // Include any additional config from environmental variables.
    // This will override the options above if necessary,
    // so e.g. it is still possible to get full debug output from hyper.
    if let Ok(ref conf) = env::var("RUST_LOG") {
        builder.parse(conf);
    }

    // Initialize the logger, possibly logging the excessive verbosity option.
    try!(builder.init());
    if excessive {
        warn!("-v/-q flag passed too many times, logging level {} assumed", level);
    }
    Ok(())
}

/// Format a single logging message using the metadata (log level etc.).
fn format_log_record(record: &LogRecord) -> String {
    if record.level() >= LogLevel::Debug {
        // TODO: include timestamp
        let location = record.location();
        format!("{} {}#{}] {}",
            record.level(), location.module_path(), location.line(), record.args())
    } else {
        format!("{} {}", record.level(), record.args())
    }
}
