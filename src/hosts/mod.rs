//! Module defining gist hosts.
//!
//! A host is an external (web) service that hosts gists, and allows users to paste snippets
//! of code to share with others. gist.github.com is a prime example; others are the various
//! "pastebins", including the pastebin.com namesake.

mod common;

mod github;
mod bpaste;
mod hastebin;
mod heypasteit;
mod lpaste;
mod mozilla;
mod paste;
mod pastebin;


use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use super::gist::{self, Gist};


/// Represents a gists' host: a (web) service that hosts gists (code snippets).
/// Examples include gist.github.com.
pub trait Host : Send + Sync {
    /// Returns a unique identifier of the gist Host.
    fn id(&self) -> &'static str;
    /// Returns a user-visible name of the gists' host.
    fn name(&self) -> &str;

    /// Fetch a current version of the gist if necessary.
    ///
    /// The `mode` parameter specifies in what circumstances the gist will be fetched
    /// from the remote host: always, only if new, or when needed.
    ///
    /// If the gist has been downloaded previously,
    /// it can also be updated instead (e.g. via pull rather than clone
    /// if its a Git repo).
    fn fetch_gist(&self, gist: &Gist, mode: FetchMode) -> io::Result<()>;

    /// Return a URL to a HTML page that can display the gist.
    /// This may involve talking to the remote host.
    fn gist_url(&self, gist: &Gist) -> io::Result<String>;

    /// Return a structure with information/metadata about the gist.
    ///
    /// Note: The return type for this method is io::Result<Option<Info>>
    /// rather than Option<io::Result<Info>> because the availability of
    /// gist metadata may be gist-specific (i.e. some gists have it,
    /// some don't).
    fn gist_info(&self, _: &Gist) -> io::Result<Option<gist::Info>> {
        // This default indicates the host doesn't expose any gist metadata.
        Ok(None)
    }

    /// Return a gist corresponding to the given URL.
    /// The URL will typically point to a user-facing HTML page of the gist.
    ///
    /// Note: The return type of this method is an Option (Option<io::Result<Gist>>)
    /// because the URL may not be recognized as belonging to this host.
    fn resolve_url(&self, _: &str) -> Option<io::Result<Gist>> {
        // This default indicates that the URL wasn't recognized
        // as pointing to any gist hosted by this host.
        None
    }
}

custom_derive! {
    #[derive(Clone, Debug, PartialEq, Eq, Hash,
             IterVariants(FetchModes))]
    pub enum FetchMode {
        /// Automatically decide how & whether to fetch the gist.
        ///
        /// This is host-specific, but should typically mean that the gist
        /// is only updated periodically, or when it's necessary to do so.
        Auto,
        /// Always fetch the gist from the remote host.
        Always,
        /// Only fetch the gist if necessary
        /// (i.e. when it hasn't been downloaded before).
        New,
    }
}
impl Default for FetchMode {
    #[inline]
    fn default() -> Self { FetchMode::Auto }
}


/// Mapping of gist host identifiers to Host structs.
lazy_static! {
    static ref BUILTIN_HOSTS: HashMap<&'static str, Arc<Host>> = hashmap!{
        github::ID => Arc::new(github::GitHub::new()) as Arc<Host>,
        pastebin::ID => Arc::new(pastebin::create()) as Arc<Host>,
        lpaste::ID => Arc::new(lpaste::create()) as Arc<Host>,
        heypasteit::ID => Arc::new(heypasteit::create()) as Arc<Host>,
        bpaste::ID => Arc::new(bpaste::create()) as Arc<Host>,
        mozilla::ID => Arc::new(mozilla::create()) as Arc<Host>,
        paste::ID => Arc::new(paste::create()) as Arc<Host>,
        hastebin::ID => Arc::new(hastebin::Hastebin::new()) as Arc<Host>,
    };
}
#[cfg(not(test))]
lazy_static! {
    pub static ref HOSTS: HashMap<&'static str, Arc<Host>> = BUILTIN_HOSTS.clone();
}
#[cfg(test)]
lazy_static! {
    pub static ref HOSTS: HashMap<&'static str, Arc<Host>> = {
        use util::testing::{DEFAULT_IN_MEMORY_HOST_ID, InMemoryHost};
        let mut hosts = BUILTIN_HOSTS.clone();
        hosts.insert(DEFAULT_IN_MEMORY_HOST_ID, Arc::new(InMemoryHost::new()) as Arc<Host>);
        hosts
    };
}

pub const DEFAULT_HOST_ID: &'static str = github::ID;


#[cfg(test)]
mod tests {
    use super::{DEFAULT_HOST_ID, HOSTS};

    #[test]
    fn consistent_hosts() {
        for (&id, host) in &*HOSTS {
            assert_eq!(id, host.id());
        }
    }

    #[test]
    fn default_host_id() {
        assert!(HOSTS.contains_key(DEFAULT_HOST_ID),
            "Default host ID `{}` doesn't occur among known gist hosts", DEFAULT_HOST_ID);
    }
}
