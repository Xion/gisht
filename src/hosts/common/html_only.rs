//! Module implementing an HTML-only gist host.

use std::error::Error;
use std::io::{self, Read};

use antidote::Mutex;
use hyper::header::UserAgent;
use regex::Regex;
use select::document::Document;
use select::predicate::Predicate;

use ::USER_AGENT;
use gist::Gist;
use hosts::{FetchMode, Host};
use util::{http_client, LINESEP};
use super::util::{ID_PLACEHOLDER, ImmutableGistHandler};


/// HTML-only gist host.
///
/// As the name indicates, those hosts offer only the HTML versions of gists,
/// requiring us to do some gymnastics in order to extract their raw content.
///
/// Other than that, they are very similar to `Basic` hosts:
/// gists are only downloaded once, not updated, consist of a single file, etc.
#[derive(Debug)]
#[cfg_attr(test, repr(C))]  // TODO: remove when `impl Trait` is stable
pub struct HtmlOnly<P: Predicate + Clone + Send> {
    /// Helper object for handling URL & gist resolve logic.
    handler: ImmutableGistHandler,
    /// Predicate for finding gist code in the HTML page.
    code_predicate: Mutex<P>,  // for Host: Send + Sync
}

impl<P: Predicate + Clone + Send> HtmlOnly<P> {
    // TODO: use the builder pattern
    pub fn new(id: &'static str,
               name: &'static str,
               html_url_pattern: &'static str,
               gist_id_re: Regex,
               code_predicate: P) -> Result<Self, Box<Error>> {
        let handler =
            ImmutableGistHandler::new(id, name, html_url_pattern, gist_id_re)?;
        Ok(HtmlOnly {
            handler,
            code_predicate: Mutex::new(code_predicate),
        })
    }
}

// Accessors / getters, used for testing of individual host setups.
#[cfg(test)]
impl<P: Predicate + Clone + Send> HtmlOnly<P> {
    pub fn html_url_regex(&self) -> &Regex { &self.handler.html_url_re }

    /// Returns the scheme + domain part of HTML URLs, like: http://example.com
    pub fn html_url_origin(&self) -> String {
        use url::Url;
        Url::parse(self.handler.html_url_pattern).unwrap()
            .origin().unicode_serialization()
    }
}

impl<P: Predicate + Clone + Send> Host for HtmlOnly<P> {
    fn id(&self) -> &'static str { self.handler.host_id }
    fn name(&self) -> &'static str { self.handler.host_name }

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
impl<P: Predicate + Clone + Send> HtmlOnly<P> {
    /// Download given gist.
    ///
    /// The gist is downloaded from the HTML URL and its code is extracted
    /// using the stored HTML predicate.
    fn download_gist(&self, gist: &Gist) -> io::Result<()> {
        let http = http_client();

        // Download the gist using the HTML URL pattern.
        let url = self.handler.html_url_pattern
            .replace(ID_PLACEHOLDER, gist.id.as_ref().unwrap());
        debug!("Downloading {} gist from {}", self.name(), url);
        let mut resp = try!(http.get(&url)
            .header(UserAgent(USER_AGENT.clone()))
            .send()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

        let mut html = String::new();
        resp.read_to_string(&mut html)?;

        // Get the HTML nodes matching the predicate and concatenate their text content.
        let document = Document::from(html.as_str());
        let mut code = document.find(self.code_predicate.lock().clone())
            .fold(String::new(), |mut s, node| {
                s.push_str(node.text().as_str()); s
            });

        // Ensure it ends with a newline, avoiding reallocation if possible.
        if !code.ends_with(LINESEP) {
            let code_len = code.len();
            if code_len - code.trim_right().len() >= LINESEP.len() {
                // TODO: replace with String::splice when it's stable
                unsafe {
                    code[code_len - LINESEP.len()..].as_bytes_mut()
                       .copy_from_slice(LINESEP.as_bytes());
                }
            } else {
                code = code.trim_right().to_owned() + LINESEP;
            }
        }

        self.handler.store_gist(gist, code.as_bytes())?;
        Ok(())
    }
}
