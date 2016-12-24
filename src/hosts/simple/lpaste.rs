//! Module implementing lpaste.net as simple gist host.

use regex::Regex;

use super::Simple;


/// lpaste.net host ID.
pub const ID: &'static str = "lp";

/// Create the lpaste.net Host implementation.
pub fn create() -> Simple {
    Simple::new(ID, "lpaste.net",
                "http://lpaste.net/raw/${id}",
                "http://lpaste.net/${id}",
                Regex::new("[0-9]+").unwrap())
}


// TODO: tests for html_url_regex
