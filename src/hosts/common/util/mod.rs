//! Utility code shared by common gist host implementations.

pub mod snippet_handler;


use std::error::Error;

use url::Url;


/// Placeholder for gist IDs in URL patterns.
pub const ID_PLACEHOLDER: &'static str = "${id}";

// Known HTTP protocol prefixes.
const HTTP: &'static str = "http://";
const HTTPS: &'static str = "https://";


/// Check the HTML URL pattern to see if it's valid.
pub fn validate_url_pattern(pattern: &'static str) -> Result<(), Box<Error>> {
    try!(Url::parse(pattern)
        .map_err(|e| format!("`{}` is not a valid URL: {}", pattern, e)));

    if ![HTTP, HTTPS].iter().any(|p| pattern.starts_with(p)) {
        return Err(format!(
            "URL pattern `{}` doesn't start with a known HTTP protocol",
            pattern).into());
    }
    if !pattern.contains(ID_PLACEHOLDER) {
        return Err(format!(
            "URL pattern `{}` does not contain the ID placeholder `{}`",
            pattern, ID_PLACEHOLDER).into());
    }

    Ok(())
}
