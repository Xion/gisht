//! Module implementing the handling of gists.
//!
//! Gists are represented as the Gist structure, with the auxiliary URI
//! that helps refering to them as command line arguments to the program.

mod info;
mod uri;


use std::borrow::Cow;
use std::path::PathBuf;

use super::{BIN_DIR, GISTS_DIR};
pub use self::info::{Datum, Info, InfoBuilder};
pub use self::uri::{Uri, UriError};


/// Structure representing a single gist.
#[derive(Debug, Clone)]
pub struct Gist {
    /// URI to the gist.
    pub uri: Uri,
    /// Alternative, host-specific ID of the gist.
    pub id: Option<String>,
    /// Optional gist info, which may be available.
    ///
    /// Note that this can be None or partial.
    /// No piece of gist info is guaranteed to be available.
    pub info: Option<Info>,
}

impl Gist {
    #[inline]
    pub fn new<I: ToString>(uri: Uri, id: I) -> Gist {
        Gist{uri: uri, id: Some(id.to_string()), info: None}
    }

    #[inline]
    pub fn from_uri(uri: Uri) -> Self {
        Gist{uri: uri, id: None, info: None}
    }

    /// Create the copy of Gist that has given ID attached.
    #[inline]
    pub fn with_id<S: ToString>(self, id: S) -> Self {
        Gist{id: Some(id.to_string()), ..self}
    }

    /// Create a copy of Gist with given gist Info attached.
    /// Note that two Gists are considered identical if they only differ by Info.
    #[inline]
    pub fn with_info(self, info: Info) -> Self {
        Gist{info: Some(info), ..self}
    }
}

impl Gist {
    /// Returns the path to this gist in the local gists directory
    /// (regardless whether it was downloaded or not).
    pub fn path(&self) -> PathBuf {
        // If the gist is identified by a host-specific ID, it should be a part of the path
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
    #[inline]
    pub fn binary_path(&self) -> PathBuf {
        let uri_path: PathBuf = self.uri.clone().into();
        BIN_DIR.join(uri_path)
    }

    /// Whether the gist has been downloaded previously.
    #[inline]
    pub fn is_local(&self) -> bool {
        // Path::exists() will traverse symlinks, so this also ensures
        // that the target "binary" file of the gist exists.
        self.binary_path().exists()
    }

    /// Retrieve a specific piece of gist Info, if available.
    #[inline]
    pub fn info(&self, datum: Datum) -> Option<info::Value> {
        let info = try_opt!(self.info.as_ref());
        if info.has(datum) {
            Some(info.get(datum).into_owned())
        } else {
            None
        }
    }

    /// Get an InfoBuilder based on this gist's Info (if any).
    #[inline]
    pub fn info_builder(&self) -> InfoBuilder {
        self.info.clone().map(|i| i.to_builder()).unwrap_or_else(InfoBuilder::new)
    }

    /// Retrieve the main language this gist has been written in, if known.
    pub fn main_language(&self) -> Option<&str> {
        let info = try_opt!(self.info.as_ref());

        // To be able to return Option<&str> rather than Option<String>,
        // we need to get the underlying reference from Cow returned by Info::get.
        let csv_langs = match info.get(Datum::Language) {
            Cow::Borrowed(lang) => lang,
            _ => return None,  // Language field is default/unknown.
        };
        csv_langs.split(",").map(|l| l.trim()).next()
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


#[cfg(test)]
mod tests {
    use gist::Uri;
    use hosts;
    use super::Gist;

    const HOST_ID: &'static str = hosts::DEFAULT_HOST_ID;
    const OWNER: &'static str = "JohnDoe";
    const NAME: &'static str = "foo";
    const ID: &'static str = "1234abcd5678efgh";

    #[test]
    fn path_without_id() {
        let gist = Gist::from_uri(Uri::new(HOST_ID, OWNER, NAME).unwrap());
        let path = gist.path().to_str().unwrap().to_owned();
        assert!(path.contains(HOST_ID), "Gist path should contain host ID");
        assert!(path.contains(OWNER), "Gist path should contain owner");
        assert!(path.contains(NAME), "Gist path should contain gist name");
    }

    #[test]
    fn path_with_id() {
        let gist = Gist::from_uri(Uri::from_name(HOST_ID, NAME).unwrap())
            .with_id(ID);
        let path = gist.path().to_str().unwrap().to_owned();
        assert!(path.contains(HOST_ID), "Gist path should contain host ID");
        assert!(path.contains(ID), "Gist path should contain gist ID");
        assert!(!path.contains(NAME), "Gist path shouldn't contain gist name");
    }

    #[test]
    fn binary_path() {
        let gist = Gist::from_uri(Uri::new(HOST_ID, OWNER, NAME).unwrap());
        let path = gist.binary_path().to_str().unwrap().to_owned();
        assert!(path.contains(HOST_ID), "Gist binary path should contain host ID");
        assert!(path.contains(OWNER), "Gist binary path should contain owner");
        assert!(path.contains(NAME), "Gist binary path should contain gist name");
    }
}
