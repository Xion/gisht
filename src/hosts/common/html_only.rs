//! Module implementing an HTML-only gist host.

use std::borrow::Cow;
use std::error::Error;
use std::fs;
use std::io::{self, Read, Write};

use antidote::Mutex;
use hyper::header::UserAgent;
use regex::{self, Regex};
use select::document::Document;
use select::predicate::Predicate;

use ::USER_AGENT;
use gist::{self, Gist};
use hosts::{FetchMode, Host};
use util::{http_client, mark_executable, symlink_file};
use super::util::{ID_PLACEHOLDER, HTTP, HTTPS, validate_url_pattern};


// TODO: large swaths of the code here have been copied from Basic --
// the only thing that really differs is HtmlOnly::download_gist;
// we could encapsulate the concept of single-file gist that's never updated
// in a separate type that both Basic & HtmlOnly would use,
// and that would contain the logic for resolving HTML URLs and gists
// (something like SingleFileGistResolver).


/// HTML-only gist host.
///
/// As the name indicates, those hosts offer only the HTML versions of gists,
/// requiring us to do some gymnastics in order to extract their raw content.
///
/// Other than that, they are very similar to `Basic` hosts:
/// gists are only downloaded once, not updated, consist of a single file, etc.
#[derive(Debug)]
pub struct HtmlOnly<P: Predicate + Clone + Send> {
    /// ID of the gist host.
    id: &'static str,
    /// User-visible name of the gist host.
    name: &'static str,
    /// Pattern for URLs pointing to browser pages of gists.
    html_url_pattern: &'static str,
    /// Regular expression for recognizing browser URLs
    html_url_re: Regex,
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
        try!(validate_url_pattern(html_url_pattern));

        // Create regex for matching HTML URL by replacing the ID placeholder
        // with a named capture group.
        let html_url_re = format!("^{}$",
            regex::escape(html_url_pattern).replace(
                &regex::escape(ID_PLACEHOLDER), &format!("(?P<id>{})", gist_id_re.as_str())));

        Ok(HtmlOnly {
            id: id,
            name: name,
            html_url_pattern: html_url_pattern,
            html_url_re: try!(Regex::new(&html_url_re)),
            code_predicate: Mutex::new(code_predicate),
        })
    }
}

// Accessors / getters, used for testing of individual host setups.
#[cfg(test)]
impl<P: Predicate + Clone + Send> HtmlOnly<P> {
    pub fn html_url_regex(&self) -> &Regex { &self.html_url_re }

    /// Returns the scheme + domain part of HTML URLs, like: http://example.com
    pub fn html_url_origin(&self) -> String {
        use url::Url;
        Url::parse(self.html_url_pattern).unwrap().origin().unicode_serialization()
    }
}

impl<P: Predicate + Clone + Send> Host for HtmlOnly<P> {
    fn id(&self) -> &'static str { self.id }
    fn name(&self) -> &'static str { self.name }

    /// Fetch the gist content from remote host
    /// and crate the appropriate binary symlink.
    fn fetch_gist(&self, gist: &Gist, mode: FetchMode) -> io::Result<()> {
        try!(self.ensure_host_id(gist));
        let gist = self.resolve_gist(gist);

        // Because the gist is only downloaded once and not updated,
        // the only fetch mode that matters is Always, which forces a re-download.
        // In other cases, a local gist is never fetched again.
        if mode != FetchMode::Always && gist.is_local() {
            debug!("Gist {} already downloaded", gist.uri);
        } else {
            if mode == FetchMode::Always {
                trace!("Forcing download of gist {}", gist.uri);
            } else {
                trace!("Gist {} needs to be downloaded", gist.uri);
            }
            try!(self.download_gist(&*gist));
        }

        Ok(())
    }

    /// Return the URL to gist's HTML website.
    fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        try!(self.ensure_host_id(gist));
        let gist = self.resolve_gist(gist);

        trace!("Building URL for {:?}", gist);
        let url = self.html_url_pattern.replace(ID_PLACEHOLDER, gist.id.as_ref().unwrap());
        debug!("Browser URL for {:?}: {}", gist, url);
        Ok(url)
    }

    /// Return a Gist based on URL to a paste's browser website.
    fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
        trace!("Checking if `{}` is a {} gist URL", url, self.name);

        // Clean up the URL a little,
        let orig_url = url.to_owned();
        let url = self.sanitize_url(url);

        // Check if it matches the pattern of gist's page URLs.
        trace!("Matching sanitized URL {} against the regex: {}",
            url, self.html_url_re.as_str());
        let captures = match self.html_url_re.captures(&*url) {
            Some(c) => c,
            None => {
                debug!("URL {} doesn't point to a {} gist", orig_url, self.name);
                return None;
            },
        };

        let id = &captures["id"];
        debug!("URL {} points to a {} gist: ID={}", orig_url, self.name, id);

        // Return the resolved gist.
        // In the gist URI, the ID is also used as name, since basic gists
        // do not have an independent, user-provided name.
        let uri = gist::Uri::from_name(self.id, id).unwrap();
        let gist = Gist::from_uri(uri).with_id(id);
        trace!("URL resolves to {} gist {} (ID={})",
            self.name, gist.uri, gist.id.as_ref().unwrap());
        Some(Ok(gist))
    }
}

// Fetching gists.
impl<P: Predicate + Clone + Send> HtmlOnly<P> {
    /// Return a "resolved" Gist that has the host ID associated with it.
    fn resolve_gist<'g>(&self, gist: &'g Gist) -> Cow<'g, Gist> {
        debug!("Resolving {} gist: {:?}", self.name, gist);
        let gist = Cow::Borrowed(gist);
        match gist.id {
            Some(_) => gist,
            None => {
                // Basic gists do actually contain the ID, but it's parsed as `name` part
                // of the URI. (The gists do not have independent, user-provided names).
                // So all we need to do is to just copy that ID.
                let id = gist.uri.name.clone();
                Cow::Owned(gist.into_owned().with_id(id))
            },
        }
    }

    /// Download given gist.
    /// The gist is downloaded from the HTML URL and its code is extracted
    /// using the stored HTML predicate.
    fn download_gist<'g>(&self, gist: &'g Gist) -> io::Result<&'g Gist> {
        let http = http_client();

        // Download the gist using the HTML URL pattern.
        let url = self.html_url_pattern.replace(ID_PLACEHOLDER, gist.id.as_ref().unwrap());
        debug!("Downloading {} gist from {}", self.name, url);
        let mut resp = try!(http.get(&url)
            .header(UserAgent(USER_AGENT.clone()))
            .send()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

        let mut html = String::new();
        resp.read_to_string(&mut html)?;

        // Get the HTML nodes matching the predicate and concatenate their text content.
        let document = Document::from(html.as_str());
        let code = document.find(self.code_predicate.lock().clone())
            .fold(String::new(), |mut s, node| {
                s.push_str(node.text().as_str()); s
            });

        // Save it under the gist path.
        // Note that Gist::path for basic gists points to a single file, not a directory,
        // so we need to ensure its *parent* exists.
        let path = gist.path();
        debug!("Saving gist {} as {}", gist.uri, path.display());
        try!(fs::create_dir_all(path.parent().unwrap()));
        let mut file = try!(fs::OpenOptions::new()
            .create(true).write(true).truncate(true)
            .open(&path));
        write!(&mut file, "{}", code)?;

        // Make sure the gist's executable is, in fact, executable.
        let executable = path;
        try!(mark_executable(&executable));
        trace!("Marked gist file as executable: {}", executable.display());

        // Create a symlink in the binary directory.
        let binary = gist.binary_path();
        if !binary.exists() {
            try!(fs::create_dir_all(binary.parent().unwrap()));
            try!(symlink_file(&executable, &binary));
            trace!("Created symlink to gist executable: {}", binary.display());
        }

        Ok(gist)
    }
}

// Resolving gist URLs.
impl<P: Predicate + Clone + Send> HtmlOnly<P> {
    fn sanitize_url<'u>(&self, url: &'u str) -> Cow<'u, str> {
        let mut url = Cow::Borrowed(url.trim());

        // Convert between HTTPS and HTTP if necessary.
        let (canonical_proto, other_http_proto);
        if self.html_url_pattern.starts_with(HTTP) {
            canonical_proto = HTTP;
            other_http_proto = HTTPS;
        } else {
            assert!(self.html_url_pattern.starts_with(HTTPS));
            canonical_proto = HTTPS;
            other_http_proto = HTTPS;
        }
        url = if url.starts_with(other_http_proto) {
            format!("{}{}", canonical_proto, url.trim_left_matches(other_http_proto)).into()
        } else {
            url.into()
        };

        // Add or remove "www".
        let canonical_has_www = self.html_url_pattern.contains("://www.");
        let input_has_www = url.contains("://www");
        if canonical_has_www != input_has_www {
            url = if canonical_has_www {
                url.replace("://", "://www.").into()
            } else {
                url.replace("://www.", "://").into()
            };
        };

        url
    }
}

// Other utility methods.
impl<P: Predicate + Clone + Send> HtmlOnly<P> {
    /// Check if given Gist is for this host. Invoke using try!().
    fn ensure_host_id(&self, gist: &Gist) -> io::Result<()> {
        if gist.uri.host_id != self.id {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!(
                "expected a {} gist, but got a '{}' one", self.name, gist.uri.host_id)));
        }
        Ok(())
    }
}
