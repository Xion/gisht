//! Module defining gist hosts.
//!
//! A host is an external (web) service that hosts gists, and allows users to paste snippets
//! of code to share with others. gist.github.com is a prime example; others are the various
//! "pastebins", including the pastebin.com namesake.

mod github;


use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use super::gist::{self, Gist};


/// Represents a gists' host: a (web) service that hosts gists (code snippets).
/// Examples include gist.github.com.
pub trait Host : Send + Sync {
    // Returns a user-visible name of the gists' host.
    fn name(&self) -> &str;

    /// Fetch a current version of the gist.
    ///
    /// If the gist has been downloaded previously,
    /// it may be updated instead (e.g. via pull rather than clone
    /// if its a Git repo).
    fn fetch_gist(&self, gist: &Gist) -> io::Result<()>;

    /// Return a URL to a HTML page that can display the gist.
    /// This may involving talking to the remote host.
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
}


lazy_static! {
    /// Mapping of gist host identifiers (like "gh") to Host structs.
    pub static ref HOSTS: HashMap<&'static str, Arc<Host>> = hashmap!{
        github::ID => Arc::new(github::GitHub::new()) as Arc<Host>,
    };
}

pub const DEFAULT_HOST_ID: &'static str = github::ID;
