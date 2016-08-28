//! Gist URI module.

use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use regex::Regex;

use super::hosts::{DEFAULT_HOST_ID, HOSTS};
use super::Host;


/// Gist URI: custom universal resource identifier of a single gist.
/// URIs are in the format:
///
///     gist_uri ::== [host_id ":"] [owner "/"] name
///
/// where the host_id part can be omitted to assume the default,
/// and owner can be passed on as well if the name itself is identifier enough
/// (this is usually host-specific).
#[derive(Clone)]
pub struct Uri {
    pub host_id: String,
    pub owner: String,
    pub name: String,
}

impl Uri {
    /// Construct a gist URI from given fragments.
    pub fn new<H, O, N>(host_id: H, owner: O, name: N) -> Result<Uri, UriError>
        where H: AsRef<str> + ToString, O: ToString, N: ToString
    {
        if !HOSTS.contains_key(host_id.as_ref()) {
            return Err(UriError::UnknownHost(host_id.to_string()));
        }
        Ok(Uri{
            host_id: host_id.to_string(),
            owner: owner.to_string(),
            name: name.to_string(),
        })
    }

    /// Construct a gist URI from just the host and name/ID.
    pub fn from_name<H, N>(host_id: H, name: N) -> Result<Uri, UriError>
        where H: AsRef<str> + ToString, N: ToString
    {
        Uri::new(host_id, "", name)
    }

    #[inline]
    pub fn has_owner(&self) -> bool { !self.owner.is_empty() }

    #[inline]
    pub fn host(&self) -> &Host {
        let host = HOSTS.get(&self.host_id as &str).unwrap(); &**host
    }
}

impl FromStr for Uri {
    type Err = UriError;

    /// Create the gist URI from its string representation.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        lazy_static!{
            static ref RE: Regex = Regex::new(
                r"(?P<host>(\w+):)?((?P<owner>\w+)/)?(?P<name>\w+)"
            ).unwrap();
        }
        let parsed = try!(RE.captures(s)
            .ok_or_else(|| UriError::Malformed(s.to_owned())));
        Uri::new(parsed.name("host").unwrap_or(DEFAULT_HOST_ID),
                 parsed.name("owner").unwrap_or(""),
                 parsed.name("name").unwrap())
    }
}

impl Into<PathBuf> for Uri {
    fn into(self) -> PathBuf {
        let has_owner = self.has_owner();
        let mut path = PathBuf::new();
        path.push(self.host_id);
        if has_owner {
            path.push(self.owner);
        }
        path.push(self.name);
        path

    }
}

impl fmt::Display for Uri {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}:{}/{}", self.host_id, self.owner, self.name)
    }
}

impl fmt::Debug for Uri {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Uri{{\"{}\", owner={}, name={}}}",
            self.host_id, self.owner, self.name)
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

impl Error for UriError {
    fn description(&self) -> &str { "gist URI error"}
    fn cause(&self) -> Option<&Error> { None }
}

impl fmt::Display for UriError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            UriError::Malformed(ref u) => write!(fmt, "malformed gist URI: {}", u),
            UriError::UnknownHost(ref h) => write!(fmt, "unknown gist host ID: {}", h),
        }
    }
}