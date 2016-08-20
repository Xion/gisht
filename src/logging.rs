//! Module implementing logging for the application.

use fern;
use log::{LogLevel, LogLevelFilter, LogLocation};


/// Initialize logging with given verbosity.
pub fn init(verbose: bool) -> Result<(), fern::InitError> {
    let config = fern::DispatchConfig{
        format: Box::new(format_message),
        output: vec![fern::OutputConfig::stderr()],
        level: LogLevelFilter::Trace,
    };

    let level = if verbose { LogLevelFilter::Debug } else { LogLevelFilter::Warn };
    fern::init_global_logger(config, level)
}

/// Format a single logging message using the metadata (log level etc.).
fn format_message(msg: &str, level: &LogLevel, location: &LogLocation) -> String {
    if *level >= LogLevel::Debug {
        // TODO: include timestamp
        format!("{} {}#{}] {}", level, location.module_path(), location.line(), msg)
    } else {
        format!("{} {}", level, msg)
    }
}
