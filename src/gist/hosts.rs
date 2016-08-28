//! Module defining gist hosts.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

use github::GitHub;
use super::uri::Uri;


/// Represents a gists' host: a (web) service that hosts gists (code snippets).
/// Examples include gist.github.com.
pub trait Host : Send + Sync {
    // Returns a user-visible name of the gists' host.
    fn name(&self) -> &str;
    /// List all the gists of given owner.
    // TODO: change this to  fn iter_gists(...) -> impl Iterator<Item=Uri>
    // when it's supported in stable Rust
    fn gists(&self, owner: &str) -> Vec<Uri>;
    /// Download a gist to given directory.
    /// Note that the directory may not necessarily exist.
    fn download_gist(&self, uri: Uri, dir: &Path) -> io::Result<()>;
}


lazy_static!{
    /// Mapping of gist host identifiers (like "gh") to Host structs.
    pub static ref HOSTS: HashMap<&'static str, Arc<Host>> = hashmap!{
        "gh" => Arc::new(GitHub::new()) as Arc<Host>,
    };
}

pub const DEFAULT_HOST_ID: &'static str = "gh";
