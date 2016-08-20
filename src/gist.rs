//! Module implementing the handling of gists.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::str::FromStr;

use regex::Regex;


/// Represents a gists' host: a (web) service that hosts code snippets,
/// such as gist.github.com.
#[derive(Debug)]
pub struct Host {
    /// User-visible name of gists' host.
    pub name: &'static str,
    /// Base URL path to gists.
    pub base_url: &'static str,
}

lazy_static!{
    /// Mapping of gist host identifiers (like "gh") to Host structs.
    static ref HOSTS: HashMap<&'static str, Host> = hashmap!{
        "gh" => Host{name: "GitHub", base_url: "http://gist.github.com"},
    };
}
const DEFAULT_HOST_ID: &'static str = "gh";


/// Gist URI: custom universal identifier of a single gist.
/// URIs are in the format:
///
///     gist_uri ::== [id ":"] author "/" name
///
/// where the ID part can be omitted to assume the default.
#[derive(Debug, Clone)]
pub struct Uri {
    pub host: &'static Host,
    pub author: String,
    pub name: String,
}
impl Uri {
    /// Construct a gist URI from given fragments.
    pub fn new<'h, A, N>(host: &'h str, author: A, name: N) -> Result<Uri, UriError>
        where A: ToString, N: ToString
    {
        Ok(Uri{
            host: try!(HOSTS.get(host)
                .ok_or_else(|| UriError::UnknownHost(host.to_owned()))),
            author: author.to_string(),
            name: name.to_string(),
        })
    }
}
impl FromStr for Uri {
    type Err = UriError;

    /// Create the gist URI from its string representation.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        lazy_static!{
            static ref RE: Regex = Regex::new(
                r"(?P<host>(\w+):)?(?P<author>\w+)/(?P<name>\w+)"
            ).unwrap();
        }
        let parsed = try!(RE.captures(s)
            .ok_or_else(|| UriError::Malformed(s.to_owned())));
        Uri::new(parsed.name("host").unwrap_or(DEFAULT_HOST_ID),
                 parsed.name("author").unwrap(),
                 parsed.name("name").unwrap())
    }
}

/// An error that occurred when creating a gist URI object.
#[derive(Debug)]
pub enum UriError {
    /// The URI was completely malformed (didn't match the pattern).
    /// Argument is the entire alleged URI string.
    Malformed(String),
    /// Specified gist host ID was unknown,
    UnknownHost(String),
}
impl fmt::Display for UriError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            UriError::Malformed(ref u) => write!(fmt, "malformed gist URI: {}", u),
            UriError::UnknownHost(ref h) => write!(fmt, "unknown gist host ID: {}", h),
        }
    }
}
impl Error for UriError {
    fn description(&self) -> &str { "gist URI error"}
    fn cause(&self) -> Option<&Error> { None }
}


/// Structure representing a single gist.
#[derive(Debug, Clone)]
pub struct Gist {

}
