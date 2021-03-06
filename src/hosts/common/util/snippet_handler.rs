//! Module implementing the snippet handler.

use std::borrow::Cow;
use std::error::Error;
use std::fs;
use std::io::{self, Read};

use regex::{self, Regex};

use gist::{self, Gist};
use hosts::FetchMode;
use util::{mark_executable, symlink_file};
use super::{HTTP, HTTPS, ID_PLACEHOLDER, validate_url_pattern};


/// Structure encapsulating the logic for handling a particular kind of gists
/// that we call "snippets".
///
/// Snippets are gists that:
/// * consist of a single file (that has no meaningful name on its own)
/// * which doesn't change once its posted
///
/// Those assumptions make it possible to manage the snippet as a single file
/// (rather than a directory) which doesn't ever need to be updated
/// (though FetchMode::Always can be honored).
///
/// Individual gist hosts can instantiate this structure
/// and delegate parts of their `Host` trait implementations here.
#[derive(Debug)]
pub struct SnippetHandler {
    /// ID of the gist host.
    host_id: &'static str,
    /// User-visible name of the gist host.
    host_name: &'static str,
    /// Pattern for URLs pointing to browser pages of gists.
    html_url_pattern: &'static str,
    /// Regular expression for recognizing browser URLs
    html_url_re: Regex,
}

impl SnippetHandler {
    pub fn new(host_id: &'static str,
               host_name: &'static str,
               html_url_pattern: &'static str,
               gist_id_re: Regex) -> Result<Self, Box<Error>> {
        try!(validate_url_pattern(html_url_pattern));

        // Create regex for matching HTML URL by replacing the ID placeholder
        // with a named capture group.
        let html_url_re = format!("^{}$",
            regex::escape(html_url_pattern).replace(
                &regex::escape(ID_PLACEHOLDER), &format!("(?P<id>{})", gist_id_re.as_str())));

        Ok(SnippetHandler{
            host_id,
            host_name,
            html_url_pattern,
            html_url_re: Regex::new(&html_url_re)?,
        })
    }
}

// Accessors.
impl SnippetHandler {
    #[inline]
    pub fn host_id(&self) -> &'static str { self.host_id }
    #[inline]
    pub fn host_name(&self) -> &'static str { self.host_name }
    #[inline]
    pub fn html_url_pattern(&self) -> &'static str { self.html_url_pattern }
}
#[cfg(test)]
impl SnippetHandler {
    #[inline]
    pub fn html_url_regex(&self) -> &Regex { &self.html_url_re }

    /// Returns the scheme + domain part of HTML URLs, like: http://example.com
    pub fn html_url_origin(&self) -> String {
        use url::Url;
        Url::parse(self.html_url_pattern).unwrap().origin().unicode_serialization()
    }
}

// Fetching gists.
impl SnippetHandler {
    /// Return a "resolved" Gist that has the host ID associated with it.
    pub fn resolve_gist<'g>(&self, gist: &'g Gist) -> Cow<'g, Gist> {
        debug!("Resolving {} gist: {:?}", self.host_name, gist);
        let gist = Cow::Borrowed(gist);
        match gist.id {
            Some(_) => gist,
            None => {
                // Snippets actually contain the ID, but it's parsed as `name` part
                // of the URI. (The gists do not have independent, user-provided names).
                // So all we need to do is to just copy that ID.
                let id = gist.uri.name.clone();
                Cow::Owned(gist.into_owned().with_id(id))
            },
        }
    }

    /// See if given gist needs to be downloaded.
    ///
    /// For immutable gist, this decision is pretty easy
    /// and boils down to checking if the gist has been downloaded before.
    pub fn need_fetch(&self, gist: &Gist, mode: FetchMode) -> io::Result<bool> {
        try!(self.ensure_host_id(gist));
        let gist = self.resolve_gist(gist);

        // Because the gist is only downloaded once and not updated,
        // the only fetch mode that matters is Always, which forces a re-download.
        // In other cases, a local gist is never fetched again.
        if mode != FetchMode::Always && gist.is_local() {
            debug!("Gist {} already downloaded", gist.uri);
            Ok(false)
        } else {
            if mode == FetchMode::Always {
                trace!("Forcing download of gist {}", gist.uri);
            } else {
                trace!("Gist {} needs to be downloaded", gist.uri);
            }
            Ok(true)
        }
    }

    /// Store the downloaded content of a gist in the correct place.
    /// Returns the number of bytes written.
    ///
    /// The exact means by which the gist content is obtained are specific
    /// to the particular host, so this method takes
    pub fn store_gist<R: Read>(&self, gist: &Gist, mut content: R) -> io::Result<usize> {
        // Save gist content under the gist path.
        // Note that Gist::path for single-file gists points to a file, not a directory,
        // so we need to ensure its *parent* exists.
        let path = gist.path();
        debug!("Saving gist {} as {}", gist.uri, path.display());
        try!(fs::create_dir_all(path.parent().unwrap()));
        let mut file = try!(fs::OpenOptions::new()
            .create(true).write(true).truncate(true)
            .open(&path));
        let byte_count = io::copy(&mut content, &mut file)?;
        if byte_count == 0 {
            warn!("Gist {} had zero bytes ({} is empty)", gist.uri, path.display());
        } else {
            trace!("Wrote {} byte(s) to {}", byte_count, path.display());
        }

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

        Ok(byte_count as usize)
    }
}

// Working with gist URLs.
impl SnippetHandler {
    /// Return the URL to gist's HTML website.
    /// This method can be pass-through called by Host::gist_url.
    pub fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        try!(self.ensure_host_id(gist));
        let gist = self.resolve_gist(gist);

        trace!("Building URL for {:?}", gist);
        let url = self.html_url_pattern.replace(ID_PLACEHOLDER, gist.id.as_ref().unwrap());
        debug!("Browser URL for {:?}: {}", gist, url);
        Ok(url)
    }

    /// Return a Gist based on URL to a gist's browser website.
    /// This method can be pass-through called by Host::resolve_url.
    pub fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
        trace!("Checking if `{}` is a {} gist URL", url, self.host_name);

        // Clean up the URL a little,
        let orig_url = url.to_owned();
        let url = self.canonicalize_url(url);

        // Check if it matches the pattern of gist's page URLs.
        trace!("Matching sanitized URL {} against the regex: {}",
            url, self.html_url_re.as_str());
        let captures = match self.html_url_re.captures(&*url) {
            Some(c) => c,
            None => {
                debug!("URL {} doesn't point to a {} gist", orig_url, self.host_name);
                return None;
            },
        };

        let id = &captures["id"];
        debug!("URL {} points to a {} gist: ID={}", orig_url, self.host_name, id);

        // Return the resolved gist.
        // In the gist URI, the ID is also used as name, since basic gists
        // do not have an independent, user-provided name.
        let uri = gist::Uri::from_name(self.host_id, id).unwrap();
        let gist = Gist::from_uri(uri).with_id(id);
        trace!("URL resolves to {} gist {} (ID={})",
            self.host_name, gist.uri, gist.id.as_ref().unwrap());
        Some(Ok(gist))
    }

    /// Make given URL resemble the gist URLs of the host
    /// which uses this instance of SnippetHandler.
    fn canonicalize_url<'u>(&self, url: &'u str) -> Cow<'u, str> {
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

        // TODO: make sure the URL ends or doesn't end with a slash,
        // depending on whether html_url_pattern does
        url
    }
}

// Other utility methods.
impl SnippetHandler {
    /// Check if given Gist is for this host. Invoke using try!().
    pub fn ensure_host_id(&self, gist: &Gist) -> io::Result<()> {
        if gist.uri.host_id != self.host_id {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!(
                "expected a {} gist, but got a '{}' one",
                self.host_name, gist.uri.host_id)));
        }
        Ok(())
    }
}
