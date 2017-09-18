//! Module defining gist hosts.
//!
//! A host is an external (web) service that hosts gists, and allows users to paste snippets
//! of code to share with others. gist.github.com is a prime example; others are the various
//! "pastebins", including the pastebin.com namesake.

mod common;

mod github;
mod bpaste;
mod dpaste_de;
mod hastebin;
mod heypasteit;
mod ix_io;
mod lpaste;
mod mibpaste;
mod mozilla;
mod paste_rs;
mod pastebin;
mod sprunge;
mod thepasteb_in;


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
    fn gist_info(&self, gist: &Gist) -> io::Result<Option<gist::Info>> {
        // This default indicates the host cannot fetch any additional gist metadata
        // (beyond what may already have been fetched when resolving gist URL).
        Ok(gist.info.clone())
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

macro_attr! {
    #[derive(Clone, Debug, PartialEq, Eq, Hash,
             IterVariants!(FetchModes))]
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
        paste_rs::ID => Arc::new(paste_rs::create()) as Arc<Host>,
        hastebin::ID => Arc::new(hastebin::Hastebin::new()) as Arc<Host>,
        mibpaste::ID => Arc::new(mibpaste::Mibpaste::new()) as Arc<Host>,
        sprunge::ID => Arc::new(sprunge::Sprunge::new()) as Arc<Host>,
        dpaste_de::ID => Arc::new(dpaste_de::create()) as Arc<Host>,
        thepasteb_in::ID => Arc::new(thepasteb_in::create()) as Arc<Host>,
        ix_io::ID => Arc::new(ix_io::Ix::new()) as Arc<Host>,
    };
}
#[cfg(not(test))]
lazy_static! {
    pub static ref HOSTS: HashMap<&'static str, Arc<Host>> = BUILTIN_HOSTS.clone();
}
#[cfg(test)]
lazy_static! {
    pub static ref HOSTS: HashMap<&'static str, Arc<Host>> = {
        use testing::{INMEMORY_HOST_DEFAULT_ID, InMemoryHost};
        let mut hosts = BUILTIN_HOSTS.clone();
        hosts.insert(INMEMORY_HOST_DEFAULT_ID, Arc::new(InMemoryHost::new()) as Arc<Host>);
        hosts
    };
}

pub const DEFAULT_HOST_ID: &'static str = github::ID;


#[cfg(test)]
mod tests {
    use testing::INMEMORY_HOST_DEFAULT_ID;
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

    #[test]
    fn inmemory_host_for_testing() {
        assert!(HOSTS.contains_key(INMEMORY_HOST_DEFAULT_ID),
            "Test in-memory host ID `{}` doesn't occur among known gist hosts", INMEMORY_HOST_DEFAULT_ID);
    }
}
