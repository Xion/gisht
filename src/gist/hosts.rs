//! Module defining gist hosts.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use github::GitHub;
use super::Gist;


/// Represents a gists' host: a (web) service that hosts gists (code snippets).
/// Examples include gist.github.com.
pub trait Host : Send + Sync {
    // Returns a user-visible name of the gists' host.
    fn name(&self) -> &str;

    /// List all the gists of given owner.
    // TODO: change this to  fn iter_gists(...) -> impl Iterator<Item=Gist>
    // when it's supported in stable Rust
    fn gists(&self, owner: &str) -> Vec<Gist>;

    /// Download a current version of the gist.
    ///
    /// If the gist has been downloaded previously,
    /// it may be updated instead (e.g. via pull rather than clone
    /// if its a Git repo).
    fn download_gist(&self, gist: &Gist) -> io::Result<()>;
}


lazy_static!{
    /// Mapping of gist host identifiers (like "gh") to Host structs.
    pub static ref HOSTS: HashMap<&'static str, Arc<Host>> = hashmap!{
        "gh" => Arc::new(GitHub::new()) as Arc<Host>,
    };
}

pub const DEFAULT_HOST_ID: &'static str = "gh";
