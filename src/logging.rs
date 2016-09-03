//! Module implementing logging for the application.

use std::env;

use env_logger::LogBuilder;
use log::{LogLevel, LogLevelFilter, LogRecord, SetLoggerError};


/// Initialize logging with given verbosity.
// TODO: more granular logging verbosity
pub fn init(verbose: bool) -> Result<(), SetLoggerError> {
    // Include universal options, like the formatting function.
    let level = if verbose { LogLevelFilter::Debug } else { LogLevelFilter::Warn };
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

    builder.init()
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
