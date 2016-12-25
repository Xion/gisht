//! Module implementing a generic simple gist host.

use std::borrow::Cow;
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;

use hyper::client::{Client, Response};
use hyper::header::UserAgent;
use regex::{self, Regex};
use url::Url;

use ::USER_AGENT;
use gist::{self, Gist};
use hosts::{FetchMode, Host};
use util::{LINESEP, mark_executable, symlink_file};


/// Placeholder for gist IDs in URL patterns.
const ID_PLACEHOLDER: &'static str = "${id}";

// Known HTTP protocol prefixes.
const HTTP: &'static str = "http://";
const HTTPS: &'static str = "https://";


/// Generic simple gist host.
///
/// This simplicity is based upon the following assumptions:
/// * Every gist consists of a single file only
/// * Gists are only downloaded once and never need to be updated
/// * Gists are identified by their ID only,
///   and their URLs are in the basic form of http://example.com/$ID.
///
/// As it turns out, a surprising number of actual gist hosts fit this description,
/// including the popular ones such pastebin.com.
#[derive(Debug)]
pub struct Simple {
    /// ID of the gist host.
    pub id: &'static str,
    /// User-visible name of the gist host.
    name: &'static str,
    /// Pattern for "raw" URLs used to download gists.
    raw_url_pattern: &'static str,
    /// Pattern for URLs pointing to browser pages of gists.
    html_url_pattern: &'static str,
    /// Regular expression for recognizing browser URLs
    html_url_re: Regex,
}

// Creation functions.
impl Simple {
    // TODO: use the Builder pattern
    pub fn new(id: &'static str,
               name: &'static str,
               raw_url_pattern: &'static str,
               html_url_pattern: &'static str,
               gist_id_re: Regex) -> Result<Self, Box<Error>> {
        try!(Self::check_url_pattern(raw_url_pattern));
        try!(Self::check_url_pattern(html_url_pattern));

        // Create regex for matching HTML URL by replacing the ID placeholder
        // with a named capture group.
        let html_url_re = format!("^{}$",
            regex::quote(html_url_pattern).replace(
                &regex::quote(ID_PLACEHOLDER), &format!("(?P<id>{})", gist_id_re.as_str())));

        Ok(Simple {
            id: id,
            name: name,
            raw_url_pattern: raw_url_pattern,
            html_url_pattern: html_url_pattern,
            html_url_re: try!(Regex::new(&html_url_re)),
        })
    }

    fn check_url_pattern(pattern: &'static str) -> Result<(), Box<Error>> {
        try!(Url::parse(pattern)
            .map_err(|e| format!("`{}` is not a valid URL: {}", pattern, e)));

        if ![HTTP, HTTPS].iter().any(|p| pattern.starts_with(p)) {
            return Err(format!(
                "URL pattern `{}` doesn't start with a known HTTP protocol",
                pattern).into());
        }
        if !pattern.contains(ID_PLACEHOLDER) {
            return Err(format!(
                "URL pattern `{}` does not contain the ID placeholder `{}`",
                pattern, ID_PLACEHOLDER).into());
        }

        Ok(())
    }
}

// Accessors / getters, used for testing of individual host setups.
#[cfg(test)]
impl Simple {
    pub fn html_url_regex(&self) -> &Regex { &self.html_url_re }

    /// Returns the scheme + domain part of HTML URLs, like: http://example.com
    pub fn html_url_origin(&self) -> String {
        Url::parse(self.html_url_pattern).unwrap().origin().unicode_serialization()
    }
}

impl Host for Simple {
    fn name(&self) -> &'static str { self.name }

    // Fetch the gist content from remote host
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

        debug!("Building URL for {:?}", gist);
        let url = self.html_url_pattern.replace(ID_PLACEHOLDER, gist.id.as_ref().unwrap());
        trace!("Browser URL for {:?}: {}", gist, url);
        Ok(url)
    }

    /// Return a Gist based on URL to a paste's browser website.
    fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
        trace!("Checking if `{}` is a {} gist URL", url, self.name);

        // Clean up the URL a little,
        let orig_url = url.to_owned();
        let url = self.sanitize_url(url);

        // Check if it matches the pattern of gist's page URLs.
        let captures = match self.html_url_re.captures(&*url) {
            Some(c) => c,
            None => {
                debug!("URL {} doesn't point to a {} gist", self.name, orig_url);
                return None;
            },
        };

        let id = captures.name("id").unwrap();
        debug!("URL {} points to a {} gist: ID={}", orig_url, self.name, id);

        // Return the resolved gist.
        // In the gist URI, the ID is also used as name, since simple gists
        // do not have an independent, user-provided name.
        let uri = gist::Uri::from_name(self.id, id).unwrap();
        let gist = Gist::from_uri(uri).with_id(id);
        trace!("URL resolves to {} gist {} (ID={})",
            self.name, gist.uri, gist.id.as_ref().unwrap());
        Some(Ok(gist))
    }
}

// Fetching gists.
impl Simple {
    /// Return a "resolved" Gist that has the host ID associated with it.
    fn resolve_gist<'g>(&self, gist: &'g Gist) -> Cow<'g, Gist> {
        debug!("Resolving {} gist: {:?}", self.name, gist);
        let gist = Cow::Borrowed(gist);
        match gist.id {
            Some(_) => gist,
            None => {
                // Simple gists do actually contain the ID, but it's parsed as `name` part
                // of the URI. (The gists do not have independent, user-provided names).
                // So all we need to do is to just copy that ID.
                let id = gist.uri.name.clone();
                Cow::Owned(gist.into_owned().with_id(id))
            },
        }
    }

    /// Download given gist.
    fn download_gist<'g>(&self, gist: &'g Gist) -> io::Result<&'g Gist> {
        let http = Client::new();

        // Download the gist using the raw URL pattern.
        let url = self.raw_url_pattern.replace(ID_PLACEHOLDER, gist.id.as_ref().unwrap());
        debug!("Downloading {} gist from {}", self.name, url);
        let mut resp = http.get(&url)
            .header(UserAgent(USER_AGENT.clone()))
            .send().unwrap();

        // Save it under the gist path.
        // Note that Gist::path for simple gists points to a single file, not a directory,
        // so we need to ensure its *parent* exists.
        let path = gist.path();
        debug!("Saving gist {} as {}", gist.uri, path.display());
        try!(fs::create_dir_all(path.parent().unwrap()));
        try!(write_http_response_file(&mut resp, &path));

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
impl Simple {
    fn sanitize_url<'u>(&self, url: &'u str) -> Cow<'u, str> {
        let mut url = Cow::Borrowed(url.trim().trim_right_matches("/"));

        //  Convert between HTTPS and HTTP if necessary.
        let (canonical_proto, other_http_proto);
        if self.html_url_pattern.starts_with(HTTP) {
            canonical_proto = HTTP;
            other_http_proto = HTTPS;
        } else {
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
impl Simple {
    /// Check if given Gist is for this host. Invoke using try!().
    fn ensure_host_id(&self, gist: &Gist) -> io::Result<()> {
        if gist.uri.host_id != self.id {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!(
                "expected a {} gist, but got a '{}' one", self.name, gist.uri.host_id)));
        }
        Ok(())
    }
}

// Utility functions

/// Write an HTTP response to a file.
/// If the file exists, it is overwritten.
fn write_http_response_file<P: AsRef<Path>>(response: &mut Response, path: P) -> io::Result<()> {
    let path = path.as_ref();
    let mut file = try!(fs::OpenOptions::new()
        .create(true).write(true).truncate(true)
        .open(path));

    // Read the response line-by-line and write it to the file
    // with an OS-specific line separator.
    let reader = BufReader::new(response);
    let mut line_count = 0;
    for line in reader.lines() {
        let line = try!(line);
        try!(file.write_fmt(format_args!("{}{}", line, LINESEP)));
        line_count += 1;
    }

    trace!("Wrote {} line(s) to {}", line_count, path.display());
    Ok(())
}


#[cfg(test)]
mod tests {
    use regex::Regex;
    use super::Simple;

    const ID: &'static str = "foo";
    const NAME: &'static str = "Foo";
    lazy_static! {
        static ref ID_RE: Regex = Regex::new(r"\w+").unwrap();
    }

    #[test]
    fn invalid_raw_url() {
        let error = Simple::new(
            ID, NAME, "invalid", "http://example.com/${id}", ID_RE.clone()).unwrap_err();
        assert!(format!("{}", error).contains("URL"));

        let error = Simple::new(ID, NAME,
                                "http://example.com/nolaceholder",
                                "http://example.com/${id}", ID_RE.clone()).unwrap_err();
        assert!(format!("{}", error).contains("placeholder"));
    }

    #[test]
    fn invalid_html_url() {
        let error = Simple::new(
            ID, NAME, "http://example.com/${id}", "invalid", ID_RE.clone()).unwrap_err();
        assert!(format!("{}", error).contains("URL"));

        let error = Simple::new(ID, NAME,
                                "http://example.com/${id}",
                                "http://example.com/nolaceholder", ID_RE.clone()).unwrap_err();
        assert!(format!("{}", error).contains("placeholder"));
    }
}
