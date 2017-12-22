//! Utility code shared by common gist host implementations.

use std::error::Error;

use url::Url;


/// Placeholder for gist IDs in URL patterns.
pub const ID_PLACEHOLDER: &'static str = "${id}";

// Known HTTP protocol prefixes.
pub const HTTP: &'static str = "http://";
pub const HTTPS: &'static str = "https://";


/// Check a URL pattern to see if expected criteria.
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
