//! Module implementing pastebin.com as gist host.
//!
//! In their parlance, a single gist is a "paste".
//! Although they have an API and there are pastebin user accounts,
//! user names are not a part of pastes' URLs.

use std::borrow::Cow;
use std::fs;
use std::io::{self, Read, Write};
use std::marker::PhantomData;
use std::path::Path;

use hyper::client::{Client, Response};
use hyper::header::{ContentLength, UserAgent};
use url::Url;

use super::super::USER_AGENT;
use gist::{self, Gist};
use util::{mark_executable, symlink_file};
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
        assert!(gist.id.is_some(), "Pastebin.com paste has no ID!");

        debug!("Building URL for {:?}", gist);
        let url ={
            let mut url = Url::parse(HTML_URL).unwrap();
            url.set_path(gist.id.as_ref().unwrap());
            url.into_string()
        };
        trace!("Browser URL for {:?}: {}", gist, url);
        Ok(url)
    }

    fn resolve_url(&self, _: &str) -> Option<io::Result<Gist>> {
        None // TODO: implement
    }
}

/// Base URL to pastes' HTML pages.
const HTML_URL: &'static str = "http://pastebin.com";


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
    // TODO: pastes created on non-Linux systems may have additional characters besides \n
    // as line terminators, which screws with hashbang recognition;
    // make sure the newlines are sanitized to be OS-specific when writing the paste

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

    // Prepare a buffer for reading of the response.
    const DEFAULT_BUF_SIZE: usize = 8192;
    const MAX_BUF_SIZE: usize = 16777216;  // 16 MB
    let buf_size = match response.headers.get::<ContentLength>() {
        Some(&ContentLength(l)) => if l as usize > MAX_BUF_SIZE {
            trace!("Large HTTP response ({} bytes); using max buffer size ({} bytes)",
                l, MAX_BUF_SIZE);
            MAX_BUF_SIZE
        } else { l as usize },
        None => DEFAULT_BUF_SIZE,
    };
    let mut buffer = vec![0; buf_size];

    // Read it & write to the file.
    loop {
        let c = try!(response.read(&mut buffer));
        if c > 0 {
            trace!("Writing {} bytes to {}", c, path.display());
            try!(file.write_all(&buffer[0..c]));
        }
        if c < buf_size { break }
    }

    Ok(())
}
