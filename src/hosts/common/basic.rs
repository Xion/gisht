//! Module implementing a basic gist host.

use std::error::Error;
use std::io;

use hyper::header::UserAgent;
use regex::Regex;

use ::USER_AGENT;
use gist::Gist;
use hosts::{FetchMode, Host};
use util::http_client;
use super::util::{ID_PLACEHOLDER, validate_url_pattern};
use super::util::snippet_handler::SnippetHandler;


/// Basic gist host.
///
/// The implementation is based upon the following assumptions:
/// * Every gist consists of a single file only
/// * Gists are only downloaded once and never need to be updated
/// * Gists are identified by their ID only,
///   and their URLs are in the basic form of http://example.com/$ID.
///
/// As it turns out, a surprising number of actual gist hosts fit this description,
/// including the popular ones such pastebin.com.
#[derive(Debug)]
pub struct Basic {
    /// Helper object for handling URL & gist resolve logic.
    handler: SnippetHandler,
    /// Pattern for "raw" URLs used to download gists.
    raw_url_pattern: &'static str,
}

// Creation functions.
impl Basic {
    // TODO: use the Builder pattern
    pub fn new(id: &'static str,
               name: &'static str,
               raw_url_pattern: &'static str,
               html_url_pattern: &'static str,
               gist_id_re: Regex) -> Result<Self, Box<Error>> {
        try!(validate_url_pattern(raw_url_pattern));
        Ok(Basic {
            handler: SnippetHandler::new(id, name, html_url_pattern, gist_id_re)?,
            raw_url_pattern: raw_url_pattern,
        })
    }
}

// Accessors / getters, used for testing of individual host setups.
#[cfg(test)]
impl Basic {
    pub fn html_url_regex(&self) -> &Regex { &self.handler.html_url_regex() }

    /// Returns the scheme + domain part of HTML URLs, like: http://example.com
    pub fn html_url_origin(&self) -> String {
        self.handler.html_url_origin()
    }
}

impl Host for Basic {
    fn id(&self) -> &'static str { self.handler.host_id() }
    fn name(&self) -> &'static str { self.handler.host_name() }

    /// Fetch the gist from remote host.
    fn fetch_gist(&self, gist: &Gist, mode: FetchMode) -> io::Result<()> {
        let gist = self.handler.resolve_gist(gist);
        if self.handler.need_fetch(&*gist, mode)? {
            self.download_gist(&*gist)?;
        }
        Ok(())
    }

    /// Return the URL to gist's HTML website.
    fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        self.handler.gist_url(gist)
    }

    /// Return a Gist based on URL to a paste's browser website.
    fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
        self.handler.resolve_url(url)
    }
}

// Fetching gists.
impl Basic {
    /// Download given gist.
    fn download_gist(&self, gist: &Gist) -> io::Result<()> {
        let http = http_client();

        // Download the gist using the raw URL pattern.
        let url = self.raw_url_pattern.replace(ID_PLACEHOLDER, gist.id.as_ref().unwrap());
        debug!("Downloading {} gist from {}", self.name(), url);
        let resp = try!(http.get(&url)
            .header(UserAgent(USER_AGENT.clone()))
            .send()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

        self.handler.store_gist(gist, resp)?;
        Ok(())
    }
}


// TODO: these are really tests for ImmutableGistHandler, move them accordingly
#[cfg(test)]
mod tests {
    use regex::Regex;
    use super::Basic;

    const ID: &'static str = "foo";
    const NAME: &'static str = "Foo";
    lazy_static! {
        static ref ID_RE: Regex = Regex::new(r"\w+").unwrap();
    }

    #[test]
    fn invalid_raw_url() {
        let error = Basic::new(
            ID, NAME, "invalid", "http://example.com/${id}", ID_RE.clone()).unwrap_err();
        assert!(format!("{}", error).contains("URL"));

        let error = Basic::new(ID, NAME,
                               "http://example.com/nolaceholder",
                               "http://example.com/${id}", ID_RE.clone()).unwrap_err();
        assert!(format!("{}", error).contains("placeholder"));
    }

    #[test]
    fn invalid_html_url() {
        let error = Basic::new(
            ID, NAME, "http://example.com/${id}", "invalid", ID_RE.clone()).unwrap_err();
        assert!(format!("{}", error).contains("URL"));

        let error = Basic::new(ID, NAME,
                               "http://example.com/${id}",
                               "http://example.com/nolaceholder", ID_RE.clone()).unwrap_err();
        assert!(format!("{}", error).contains("placeholder"));
    }
}
