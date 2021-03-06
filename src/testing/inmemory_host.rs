//! Module implementing in-memory Host (a fake Host implementation for testing).

#![allow(dead_code)]

use std::io;
use std::string::FromUtf8Error;
use std::sync::RwLock;

use gist::{self, Gist};
use hosts::{FetchMode, Host};


pub const INMEMORY_HOST_DEFAULT_ID: &'static str = "mem";


/// Gist stored (or not) in the in-memory host.
struct StoredGist {
    gist: Option<Gist>,
    url: Option<String>,
    content: Option<Vec<u8>>,
}

impl StoredGist {
    #[inline]
    pub fn new(gist: Gist, url: String, content: String) -> Self {
        StoredGist{
            gist: Some(gist),
            url: Some(url),
            content: Some(content.into_bytes()),
        }
    }

    #[inline]
    pub fn with_gist(gist: Gist) -> Self {
        StoredGist{gist: Some(gist), url: None, content: None}
    }

    #[inline]
    pub fn with_gist_url(gist: Gist, url: String) -> Self {
        StoredGist{gist: Some(gist), url: Some(url), content: None}
    }

    #[inline]
    pub fn with_gist_content(gist: Gist, content: String) -> Self {
        StoredGist{
            gist: Some(gist),
            url: None,
            content: Some(content.into_bytes()),
        }
    }

    #[inline]
    pub fn with_broken_url(url: String) -> Self {
        StoredGist{gist: None, url: Some(url), content: None}
    }
}
impl From<Gist> for StoredGist {
    #[inline]
    fn from(gist: Gist) -> Self {
        StoredGist::with_gist(gist)
    }
}

impl StoredGist {
    #[inline]
    pub fn is_available(&self) -> bool {
        self.gist.is_some()
    }

    pub fn id(&self) -> Option<&str> {
        let gist = try_opt!(self.gist.as_ref());
        let id = try_opt!(gist.id.as_ref());
        Some(id.as_str())
    }

    #[inline]
    pub fn uri(&self) -> Option<&gist::Uri> {
        self.gist.as_ref().map(|g| &g.uri)
    }

    #[inline]
    pub fn content_string(&self) -> Option<Result<String, FromUtf8Error>> {
        self.content.clone().map(String::from_utf8)
    }

    #[inline]
    pub fn content_bytes(&self) -> Option<&[u8]> {
        self.content.as_ref().map(|c| &c[..])
    }
}


/// Fake implementation of a gist Host that stores gists in memory.
///
/// While it doesn't perform any disk or network I/O,
/// it uses the following formats for its URLs:
///
/// * gist URI format:: mem:$OWNER/$NAME
/// * HTML URL format:: memory://html/id/$ID or memory://html/uri/$OWNER/$NAME
///
pub struct InMemoryHost {
    id: &'static str,
    gists: RwLock<Vec<StoredGist>>,  // lock due to Host: Sync requirement
}

impl InMemoryHost {
    /// Create a default instance of in-memory host, to be accessible as standalone host.
    ///
    /// ## Warning
    ///
    /// Do not call this method in tests!
    /// The "mem" in-memory host is always accessible for crate-level and unit tests.
    pub fn new() -> Self {
        Self::with_id(INMEMORY_HOST_DEFAULT_ID)
    }

    /// Create an instance of in-memory host with given ID.
    pub fn with_id(id: &'static str) -> Self {
        InMemoryHost{
            id: id,
            gists: RwLock::new(Vec::new())
        }
    }
}

impl Host for InMemoryHost {
    fn id(&self) -> &'static str { self.id }
    fn name(&self) -> &str { "(memory)" }

    fn fetch_gist(&self, gist: &Gist, _: FetchMode) -> io::Result<()> {
        let gists = self.gists.read().unwrap();
        match gists.iter().find(|sg| sg.gist.as_ref() == Some(gist)) {
            Some(sg) => {
                if sg.content.is_some() {
                    // Gist has content, so we "downloaded" it.
                    Ok(())
                } else {
                    // This isn't something we'd expect a regular gist host to ever signal
                    // (since none would make a distinction between "no content" and
                    // "empty content"). It is however helpful in testing whether fetch_gist()
                    // has been invoked with a correct gist argument.
                    Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                        format!("{:?} doesn't contain any content", gist)))
                }
            },
            None => Err(io::Error::new(io::ErrorKind::NotFound,
                format!("Cannot find {:?}", gist))),
        }
    }

    fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        let gists = self.gists.read().unwrap();
        if let Some(stored_gist) = gists.iter().find(|sg| sg.gist.as_ref() == Some(gist)) {
            return stored_gist.url.clone()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData,
                    format!("{:?} doesn't have a URL associated with it", gist)));
        }
        Err(io::Error::new(io::ErrorKind::NotFound, format!("Cannot find {:?}", gist)))
    }

    fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
        let gists = self.gists.read().unwrap();
        let stored_gist = try_opt!(gists.iter()
            .find(|sg| sg.url.as_ref().map(String::as_str) == Some(url)));
        let result = match stored_gist.gist {
            Some(ref gist) => Ok(gist.clone()),
            None => Err(io::Error::new(io::ErrorKind::NotFound,
                format!("URL {} doesn't point to an available gist", url))),
        };
        Some(result)
    }
}

// Public interface of the in-memory gist host.
// Note that most operations are O(n) wrt to the number of gists stored.
impl InMemoryHost {
    /// Remove all stored in-memory gists.
    /// Call this at the beginning of a test.
    pub fn reset(&self) {
        let mut gists = self.gists.write().unwrap();
        gists.clear();
    }

    /// Put a gist into the collection of in-memory gists, without an associated URL.
    pub fn put_gist(&self, gist: Gist) {
        let mut gists = self.gists.write().unwrap();
        if gists.iter().find(|sg| sg.gist.as_ref() == Some(&gist)).is_some() {
            panic!("Tried to put duplicate gist {:?}", gist);
        }
        gists.push(StoredGist::from(gist));
    }

    /// Put a gist into the collection of in-memory gists with an associated URL.
    pub fn put_gist_with_url<U: ToString>(&self, gist: Gist, url: U) {
        let url = url.to_string();
        let mut gists = self.gists.write().unwrap();
        if gists.iter().find(|sg| sg.url.as_ref() == Some(&url)).is_some() {
            panic!("Tried to put gist {:?} under a duplicate URL: {}", gist, url);
        }
        gists.push(StoredGist::with_gist_url(gist, url));
    }

    /// Put a URL into gist collection that doesn't correspond to any gist.
    /// The URL will cause an error when resolved.
    pub fn put_broken_url<U: ToString>(&self, url: U) {
        let url = url.to_string();
        let mut gists = self.gists.write().unwrap();
        if gists.iter().find(|sg| sg.url.as_ref() == Some(&url)).is_some() {
            panic!("Tried to duplicate the URL: {}", url);
        }
        gists.push(StoredGist::with_broken_url(url));
    }

    /// Remove the gist from in-memory collection.
    /// Returns true if it was actually removed, false if it wasn't there.
    pub fn remove_gist_by_uri(&self, uri: &gist::Uri) -> bool {
        let mut gists = self.gists.write().unwrap();
        let maybe_idx = gists.iter().position(|sg| sg.uri() == Some(&uri));
        match maybe_idx {
            Some(idx) => { gists.remove(idx); true },
            None => false,
        }
    }

    /// Check if the in-memory collection has a gist with given ID.
    /// This is O(n) wrt to the number of stored gists.
    pub fn has_gist_for_id(&self, id: &str) -> bool {
        self.gists.read().unwrap().iter()
            .find(|sg| sg.id() == Some(id))
            .is_some()
    }

    /// Check if the in-memory collection contains a gist for given URI.
    pub fn has_gist_for_uri(&self, uri: &gist::Uri) -> bool {
        self.gists.read().unwrap().iter()
            .find(|sg| sg.uri() == Some(uri))
            .is_some()
    }

    /// Returns the number of stored gists.
    pub fn count(&self) -> usize {
        self.gists.read().unwrap().len()
    }
}
