//! Testing utilities.

use std::collections::HashMap;
use std::io;

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
    gists: HashMap<gist::Uri, Gist>,
}

impl InMemoryHost {
    pub fn new() -> Self {
        InMemoryHost{gists: HashMap::new()}
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
            return self.gists.iter()
                .find(|&(_, g)| g.id.as_ref().map(|gid| gid == id).unwrap_or(false))  // WTF RUST
                .map(|(_, g)| g.clone()).map(Ok);
        }

        // Alternatively, find it directly by the gist URI.
        if let Some(caps) = HTML_URL_FOR_URI_RE.captures(url) {
            debug!("URL {} points to an in-memory gist by its URI", url);
            let owner = caps.name("owner").unwrap();
            let name = caps.name("name").unwrap();
            let uri = gist::Uri::new(IN_MEMORY_HOST_ID, owner, name).unwrap();
            return self.gists.get(&uri).map(|g| Ok(g.clone()));
        }

        debug!("URL {} doesn't point to an in-memory gist", url);
        None
    }
}

impl InMemoryHost {
    // TODO: methods to fill in the gist collection for testing
}
