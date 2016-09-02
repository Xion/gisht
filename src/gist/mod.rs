//! Module implementing the handling of gists.
//!
//! Gists are represented as the Gist structure, with the auxiliary URI
//! that helps refering to them as command line arguments to the program.

mod uri;


use std::path::PathBuf;

use super::{BIN_DIR, GISTS_DIR};
pub use self::uri::Uri;



/// Structure representing a single gist.
#[derive(Debug, Clone, Eq)]
pub struct Gist {
    /// URI to the gist.
    pub uri: Uri,
    /// Alternative, host-specific ID of the gist.
    pub id: Option<String>,
}

impl Gist {
    #[inline]
    pub fn new<I: ToString>(uri: Uri, id: I) -> Gist {
        Gist{uri: uri, id: Some(id.to_string())}
    }

    #[inline]
    pub fn from_uri(uri: Uri) -> Self {
        Gist{uri: uri, id: None}
    }
}

impl Gist {
    /// Returns the path to this gist in the local gists directory
    /// (regardless whether it was downloaded or not).
    pub fn path(&self) -> PathBuf {
        // If the gist is idenfied by a host-specific ID, it should be a part of the path
        // (because uri.name is most likely not unique in that case).
        // Otherwise, the gist's URI will form its path.
        let path_fragment = match self.id {
            Some(ref id) => PathBuf::new().join(&self.uri.host_id).join(id),
            _ => self.uri.clone().into(),
        };
        GISTS_DIR.join(path_fragment)
    }

    /// Returns the path to the gist's binary
    /// (regardless whether it was downloaded or not).
    pub fn binary_path(&self) -> PathBuf {
        let uri_path: PathBuf = self.uri.clone().into();
        BIN_DIR.join(uri_path)
    }

    /// Whether the gist has been downloaded previously.
    pub fn is_local(&self) -> bool {
        // Path::exists() will traverse symlinks, so this also ensures
        // that the target "binary" file of the gist exists.
        self.binary_path().exists()
    }
}

impl PartialEq<Gist> for Gist {
    fn eq(&self, other: &Gist) -> bool {
        if self.uri != other.uri {
            return false;
        }
        if self.id.is_some() && self.id != other.id {
            return false;
        }
        true
    }
}
