//! Module implementing the handling of gists.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use regex::Regex;

use github::GitHub;


/// Represents a gists' host: a (web) service that hosts code snippets,
/// such as gist.github.com.
pub trait Host : Send + Sync {
    // Returns a user-visible name of the gists' host.
    fn name(&self) -> &str;
    /// List all the gists of given owner.
    // TODO: change this to  fn iter_gists(...) -> impl Iterator<Item=Uri>
    // when it's supported in stable Rust
    fn gists(&self, owner: &str) -> Vec<Uri>;
}


lazy_static!{
    /// Mapping of gist host identifiers (like "gh") to Host structs.
    static ref HOSTS: HashMap<&'static str, Arc<Host>> = hashmap!{
        "gh" => Arc::new(GitHub::new()) as Arc<Host>,
    };
}
const DEFAULT_HOST_ID: &'static str = "gh";


/// Gist URI: custom universal identifier of a single gist.
/// URIs are in the format:
///
///     gist_uri ::== [id ":"] author "/" name
///
/// where the ID part can be omitted to assume the default.
#[derive(Clone)]
pub struct Uri {
    pub host: Arc<Host>,
    pub owner: String,
    pub name: String,
}
impl Uri {
    /// Construct a gist URI from given fragments.
    pub fn new<'h, A, N>(host_id: &'h str, owner: A, name: N) -> Result<Uri, UriError>
        where A: ToString, N: ToString
    {
        let host = try!(HOSTS.get(host_id)
            .ok_or_else(|| UriError::UnknownHost(host_id.to_owned())));
        Ok(Uri{
            host: host.clone(),
            owner: owner.to_string(),
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
                r"(?P<host>(\w+):)?(?P<owner>\w+)/(?P<name>\w+)"
            ).unwrap();
        }
        let parsed = try!(RE.captures(s)
            .ok_or_else(|| UriError::Malformed(s.to_owned())));
        Uri::new(parsed.name("host").unwrap_or(DEFAULT_HOST_ID),
                 parsed.name("owner").unwrap(),
                 parsed.name("name").unwrap())
    }
}
impl fmt::Display for Uri {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}:{}/{}", self.host.name(), self.owner, self.name)
    }
}
impl fmt::Debug for Uri {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Uri{{host={}, owner={}, name={}}}",
            self.host.name(), self.owner, self.name)
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
