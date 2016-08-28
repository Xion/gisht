//! Module implementing the handling of gists.

mod hosts;
mod uri;


use std::path::PathBuf;

use super::{BIN_DIR, GISTS_DIR};
pub use self::hosts::Host;
pub use self::uri::Uri;



/// Structure representing a single gist.
#[derive(Debug, Clone)]
pub struct Gist {
    pub uri: Uri,
}

impl Gist {
    /// Returns the path to this gist in the local gists directory
    /// (regardless whether it was downloaded or not).
    pub fn path(&self) -> PathBuf {
        let uri_path: PathBuf = self.uri.clone().into();
        GISTS_DIR.join(uri_path)
    }

    /// Returns the path to the gist's binary
    /// (regardless whether it was downloaded or not).
    pub fn binary_path(&self) -> PathBuf {
        let uri_path: PathBuf = self.uri.clone().into();
        BIN_DIR.join(uri_path)
    }

    /// Whether the gist has been downloaded previously.
    pub fn is_local(&self) -> bool {
        self.path().exists()
    }
}
