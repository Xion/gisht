//! Module implementing pastebin.com as gist host.
//!
//! In their parlance, a single gist is a "paste".
//! Although they have an API and there are pastebin user accounts,
//! user names are not a part of pastes' URLs.

use std::borrow::Cow;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::marker::PhantomData;
use std::path::Path;

use hyper::client::{Client, Response};
use hyper::header::UserAgent;
use regex::{self, Regex};
use url::Url;

use super::super::USER_AGENT;
use gist::{self, Gist};
use util::{LINESEP, mark_executable, symlink_file};
use super::{FetchMode, Host};


/// pastebin.com host ID.
pub const ID: &'static str = "pb";


pub struct Pastebin {
    _marker: PhantomData<()>,
}

impl Pastebin {
    pub fn new() -> Self {
        Pastebin { _marker: PhantomData }
    }
}

impl Host for Pastebin {
    fn name(&self) -> &'static str { "pastebin.com" }

    /// Fetch the paste (gist) content from pastebin.com
    /// and crate the appropriate binary symlink.
    fn fetch_gist(&self, gist: &Gist, mode: FetchMode) -> io::Result<()> {
        try!(ensure_pastebin_paste(gist));
        let gist = resolve_gist(gist);

        // Because a paste (gist) is only downloaded once and not updated,
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
            try!(download_paste(&*gist));
        }

        Ok(())
    }

    /// Return the URL to paste's HTML website.
    fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        try!(ensure_pastebin_paste(gist));
        let gist = resolve_gist(gist);

        debug!("Building URL for {:?}", gist);
        let url ={
            let mut url = Url::parse(HTML_URL).unwrap();
            url.set_path(gist.id.as_ref().unwrap());
            url.into_string()
        };
        trace!("Browser URL for {:?}: {}", gist, url);
        Ok(url)
    }

    /// Return a Gist based on URL to a paste's browser website.
    fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
        trace!("Checking if `{}` is a Pastebin.com paste URL", url);

        // Clean up the URL a little, e.g. by converting HTTPS to HTTP
        // since this is what Pastebin.com uses by default.
        let orig_url = url.to_owned();
        let url: Cow<str> = {
            let url = url.trim().trim_right_matches("/");
            if url.starts_with("https://") {
                format!("http://{}", url.trim_left_matches("https://")).into()
            } else {
                url.into()
            }
        };

        // Check if it matches the pattern of paste's page URLs.
        let captures = match HTML_URL_RE.captures(&*url) {
            Some(c) => c,
            None => {
                debug!("URL {} doesn't point to a Pastebin.com gist", orig_url);
                return None;
            },
        };

        let id = captures.name("id").unwrap();
        debug!("URL {} points to a Pastebin.com gist: ID={}", orig_url, id);

        // Return the resolved gist.
        // In the gist URI, the ID is also used as name, since Pastebin.com gists
        // do not have an independent, user-provided name.
        let uri = gist::Uri::from_name(ID, id).unwrap();
        let gist = Gist::from_uri(uri).with_id(id);
        trace!("URL resolves to Pastebin.com gist {} (ID={})", gist.uri, gist.id.as_ref().unwrap());
        Some(Ok(gist))
    }
}

/// Base URL to pastes' HTML pages.
const HTML_URL: &'static str = "http://pastebin.com";

lazy_static! {
    /// Regular expression for parsing URLs to pastes HTML pages.
    static ref HTML_URL_RE: Regex = Regex::new(
        &format!("^{}/{}$", regex::quote(HTML_URL), r#"(?P<id>[0-9a-zA-Z]+)"#)
    ).unwrap();
}


// Fetching pastes (gists)

/// Prefix of a URL used to download pastes (gists).
const DOWNLOAD_URL_PREFIX: &'static str = "http://pastebin.com/raw/";

/// Return a "resolved" Gist that has a Pastebin ID associated with it.
fn resolve_gist(gist: &Gist) -> Cow<Gist> {
    debug!("Resolving Pastebin paste: {:?}", gist);
    let gist = Cow::Borrowed(gist);
    match gist.id {
        Some(_) => gist,
        None => {
            // Pastebin gists do actually contain the ID, but it's parsed as `name` part
            // of the URI. (The gists do not have independent, user-provided names).
            // So all we need to do is to just copy that ID.
            let id = gist.uri.name.clone();
            Cow::Owned(gist.into_owned().with_id(id))
        },
    }
}

/// Download given paste (gist).
fn download_paste(gist: &Gist) -> io::Result<&Gist> {
    let http = Client::new();

    // Download the paste from a Pastebin.com download URL.
    let url = format!("{}{}", DOWNLOAD_URL_PREFIX, gist.id.as_ref().unwrap());
    debug!("Downloading Pastebin paste from {}", url);
    let mut resp = http.get(&url)
        .header(UserAgent(USER_AGENT.clone()))
        .send().unwrap();

    // Save it under the gist path.
    // Note that Gist::path for Pastebin gists points to a single file, not a directory,
    // so we need to ensure its *parent* exists.
    let path = gist.path();
    debug!("Saving paste {} as {}", gist.uri, path.display());
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


// Utility functions

/// Check if given Gist is a pastebin.com paste. Invoke using try!().
fn ensure_pastebin_paste(gist: &Gist) -> io::Result<()> {
    if gist.uri.host_id != ID {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!(
            "expected a pastebin.com paste, but got a '{}' gist", gist.uri.host_id)));
    }
    Ok(())
}

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
    use super::{HTML_URL, HTML_URL_RE};

    #[test]
    fn html_url_regex() {
        lazy_static! {
            static ref VALID_HTML_URLS: Vec<(/* URL */ String,
                                             /* ID */ &'static str)> = vec![
                (HTML_URL.to_owned() + "/abc", "abc"),                // short
                (HTML_URL.to_owned() + "/a1b2c3d4e5", "a1b2c3d4e5"),  // long
                (HTML_URL.to_owned() + "/43ffg", "43ffg"),            // starts with digit
                (HTML_URL.to_owned() + "/46417247", "46417247"),      // only digits
                (HTML_URL.to_owned() + "/MfgT45f", "MfgT45f"),        // mixed case
            ];
            static ref INVALID_HTML_URLS: Vec<String> = vec![
                HTML_URL.to_owned() + "/a/b/c",         // too many path segments
                HTML_URL.to_owned() + "/a/",            // trailing slash
                HTML_URL.to_owned() + "//",             // ID must not be empty
                HTML_URL.to_owned() + "/",              // no ID at all
                "http://example.com/fhdFG36ok".into(),  // wrong Pastebin.com domain
                "foobar".into(),                        // not even an URL
            ];
        }
        for &(ref valid_url, id) in &*VALID_HTML_URLS {
            let captures = HTML_URL_RE.captures(valid_url)
                .expect(&format!("Paste's HTML URL was incorrectly deemed invalid: {}", valid_url));
            assert_eq!(id, captures.name("id").unwrap());
        }
        for invalid_url in &*INVALID_HTML_URLS {
            assert!(!HTML_URL_RE.is_match(invalid_url),
                "URL was incorrectly deemed a valid gist HTML URL: {}", invalid_url);
        }
    }
}
