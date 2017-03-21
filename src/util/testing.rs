//! Testing utilities.

use std::collections::HashMap;
use std::io;
use std::sync::RwLock;

use regex::Regex;

use gist::{self, Gist};
use hosts::{FetchMode, Host};


pub const IN_MEMORY_HOST_ID: &'static str = "mem";

/// Fake implementation of a gist Host that stores gists in memory.
///
/// While it doesn't perform any disk or network I/O, it uses the following formats
/// for its URLs:
///
/// * gist URI format:: mem:$OWNER/$NAME
/// * HTML URL format:: memory://html/id/$ID or memory://html/uri/$OWNER/$NAME
///
pub struct InMemoryHost {
    gists: RwLock<HashMap<gist::Uri, Gist>>,
}

impl InMemoryHost {
    pub fn new() -> Self {
        InMemoryHost{
            gists: RwLock::new(HashMap::new())
        }
    }
}

impl Host for InMemoryHost {
    fn id(&self) -> &'static str { IN_MEMORY_HOST_ID }
    fn name(&self) -> &str { "(memory)" }

    fn fetch_gist(&self, _: &Gist, _: FetchMode) -> io::Result<()> {
        // TODO: allow some gists to fail to be "downloaded"
        Ok(())
    }

    fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        Ok(format!("memory://html/{}", match gist.id {
            Some(ref id) => format!("id/{}", id),
            None => format!("uri/{}", gist.uri),
        }))
    }

    fn gist_info(&self, _: &Gist) -> io::Result<Option<gist::Info>> {
        // This default indicates the host doesn't expose any gist metadata.
        Ok(None)
    }

    fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
        lazy_static! {
            static ref HTML_URL_FOR_ID_RE: Regex = Regex::new(
                r#"memory://html/id/(?P<id>.+)"#).unwrap();
            static ref HTML_URL_FOR_URI_RE: Regex = Regex::new(
                r#"memory://html/uri/(?P<owner>[^/])/(?P<name>.+)"#).unwrap();
        }

        // Try to find a gist by ID. For that, we need to iterate over all gists.
        if let Some(caps) = HTML_URL_FOR_ID_RE.captures(url) {
            debug!("URL {} points to an in-memory gist by ID", url);
            let id = caps.name("id").unwrap();
            return self.gists.read().unwrap().iter()
                .find(|&(_, g)| g.id.as_ref().map(String::as_str) == Some(id))
                .map(|(_, g)| g.clone()).map(Ok);
        }

        // Alternatively, find it directly by the gist URI.
        if let Some(caps) = HTML_URL_FOR_URI_RE.captures(url) {
            debug!("URL {} points to an in-memory gist by its URI", url);
            let owner = caps.name("owner").unwrap();
            let name = caps.name("name").unwrap();
            let uri = gist::Uri::new(IN_MEMORY_HOST_ID, owner, name).unwrap();
            return self.gists.read().unwrap().get(&uri).map(|g| Ok(g.clone()));
        }

        debug!("URL {} doesn't point to an in-memory gist", url);
        None
    }
}

// Public interface of the in-memory gist host.
#[allow(dead_code)]
impl InMemoryHost {
    /// Remove all stored in-memory gists.
    /// Call this at the beginning of a test.
    pub fn reset(&self) {
        let mut gists = self.gists.write().unwrap();
        gists.clear();
    }

    /// Put a gist into the collection of in-memory gists.
    pub fn put_gist(&self, gist: Gist) {
        let mut gists = self.gists.write().unwrap();

        let id = gist.id.clone();
        let stored = gists.entry(gist.uri.clone()).or_insert(gist);
        if stored.id != id {
            panic!("Tried to add duplicate gist with URI {} and ID={:?},
                which is different than existing ID={:?}", stored.uri, id, stored.id);
        }
    }

    /// Remove the gist from in-memory collection.
    /// Returns true if it was actually removed, false if it wasn't there.
    pub fn remove_gist(&self, uri: gist::Uri) -> bool {
        let mut gists = self.gists.write().unwrap();
        gists.remove(&uri).is_some()
    }

    /// Check if the in-memory collection has a gist with given ID.
    /// This is O(n) wrt to the number of stored gists.
    pub fn has_gist_for_id(&self, id: &str) -> bool {
        self.gists.read().unwrap().iter()
            .find(|&(_, g)| g.id.as_ref().map(String::as_str) == Some(id))
            .is_some()
    }

    /// Check if the in-memory collection contains a gist for given URI.
    pub fn has_gist_for_uri(&self, uri: gist::Uri) -> bool {
        self.gists.read().unwrap().contains_key(&uri)
    }

    /// Returns the number of stored gists.
    pub fn count(&self) -> usize {
        self.gists.read().unwrap().len()
    }
}

