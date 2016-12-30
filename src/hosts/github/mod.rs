//! Module implementing GitHub as gist host.
//!
//! This is specifically about the gist.github.com part of GitHub,
//! NOT the actual GitHub repository hosting.

mod api;
mod git;
mod util;


use std::borrow::Cow;
use std::fs;
use std::io;
use std::iter::Iterator;
use std::marker::PhantomData;
use std::path::Path;
use std::time::{Duration, SystemTime};

use git2;
use regex::{self, Regex};
use url::Url;

use gist::{self, Datum, Gist};
use util::{mark_executable, symlink_file};
use super::{FetchMode, Host};


/// GitHub host ID.
pub const ID: &'static str = "gh";


#[derive(Debug)]
pub struct GitHub {
    _marker: PhantomData<()>,
}

impl GitHub {
    pub fn new() -> Self {
        GitHub { _marker: PhantomData }
    }
}

impl Host for GitHub {
    fn id(&self) -> &'static str { ID }
    fn name(&self) -> &str { "GitHub" }

    /// Fetch the gist's repo from GitHub & create the appropriate binary symlink.
    ///
    /// If the gist hasn't been downloaded already, a clone of the gist's Git repo is performed.
    /// Otherwise, updating the gist (if needed) is just a simple Git pull.
    fn fetch_gist(&self, gist: &Gist, mode: FetchMode) -> io::Result<()> {
        try!(ensure_github_gist(gist));
        let gist = try!(resolve_gist(gist));

        if gist.is_local() {
            let update = match mode {
                FetchMode::Auto => needs_update(&gist),
                FetchMode::Always => true,
                FetchMode::New => false,
            };
            if update {
                try!(update_gist(gist));
            } else {
                trace!("No need to update gist {}", gist.uri);
            }
        } else {
            try!(clone_gist(gist));
        }

        Ok(())
    }

    /// Return the URL to gist's HTML website.
    fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        debug!("Building URL for {:?}", gist);

        let gist = if gist.id.is_none() {
            trace!("Gist {} has no GitHub ID, attempting to resolve", gist.uri);
            try!(resolve_gist(gist))
        } else {
            trace!("Gist {} has ID={}, no need to resolve it for the URL",
                gist.uri, gist.id.as_ref().unwrap());
            Cow::Borrowed(gist)
        };

        // See if there is an existing URL included in the gist::Info.
        // Otherwise, build it manually.
        let url = gist.info(Datum::BrowserUrl).unwrap_or_else(|| {
            trace!("URL not found in gist info, building it manually");
            let mut url = Url::parse(HTML_URL).unwrap();
            url.set_path(&format!("{}/{}", gist.uri.owner, gist.id.as_ref().unwrap()));
            url.into_string()
        });
        trace!("Browser URL for {:?}: {}", gist, url);
        Ok(url)
    }

    /// Return a structure with gist metadata.
    fn gist_info(&self, gist: &Gist) -> io::Result<Option<gist::Info>> {
        try!(ensure_github_gist(gist));
        let gist = try!(resolve_gist(gist));

        let id = gist.id.as_ref().unwrap();
        let info = try!(api::get_gist_info(id));

        let result = api::build_gist_info(&info, &[]);
        Ok(Some(result))
    }

    /// Return a Gist based on URL to its browser HTML page.
    fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
        trace!("Checking if `{}` is a GitHub gist URL", url);

        // Clean up the URL a little, e.g. by converting HTTP to HTTPS.
        let orig_url = url.to_owned();
        let url: Cow<str> = {
            let url = url.trim();
            if url.starts_with("http://") {
                format!("https://{}", url.trim_left_matches("http://")).into()
            } else {
                url.into()
            }
        };

        // Check if it matches the pattern of gist page URLs.
        let captures = match HTML_URL_RE.captures(&*url) {
            Some(c) => c,
            None => {
                debug!("URL {} doesn't point to a GitHub gist", orig_url);
                return None;
            },
        };

        let id = captures.name("id").unwrap();
        debug!("URL {} points to a GitHub gist: ID={}", orig_url, id);

        // Obtain gist information using GitHub API.
        // Note that gist owner may be in the URL already, or we may need to get it
        // from gist info along with gist name.
        let info = try_some!(api::get_gist_info(id));
        let name = match api::gist_name_from_info(&info) {
            Some(name) => name,
            None => {
                warn!("GitHub gist with ID={} (URL={}) has no files", id, orig_url);
                return None;
            },
        };
        let owner = captures.name("owner").unwrap_or_else(|| api::gist_owner_from_info(&info));

        // Return the resolved gist.
        let uri = gist::Uri::new(ID, owner, name).unwrap();
        let gist = Gist::from_uri(uri).with_id(id);
        trace!("URL resolves to GitHub gist {} (ID={})", gist.uri, gist.id.as_ref().unwrap());
        Some(Ok(gist))
    }
}

/// Base URL to gist HTML pages.
const HTML_URL: &'static str = "https://gist.github.com";

lazy_static! {
    /// Regular expression for parsing URLs to gist HTML pages.
    static ref HTML_URL_RE: Regex = Regex::new(
        &format!("^{}/{}$", regex::quote(HTML_URL), r#"((?P<owner>[^/]+)/)?(?P<id>[0-9a-fA-F]+)"#)
    ).unwrap();
}


// Fetching gists

lazy_static! {
    /// Minimum interval between updating (git-pulling) of gists.
    static ref UPDATE_INTERVAL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
}


/// Return a "resolved" Gist that has a GitHub ID associated with it.
fn resolve_gist(gist: &Gist) -> io::Result<Cow<Gist>> {
    trace!("Resolving GitHub gist: {}", gist.uri);
    let gist = Cow::Borrowed(gist);
    if gist.id.is_some() {
        return Ok(gist);
    }

    // TODO: copy over gist.info if it's there

    // If the gist doesn't have the ID associated with it,
    // resolve the owner/name by either checking the already existing,
    // local gist, or listing all the owner's gists to find the matching ID.
    if gist.is_local() {
        let id = try!(id_from_binary_path(gist.binary_path()));
        debug!("Gist {} found locally with ID={}", gist.uri, id);
        Ok(Cow::Owned(gist.into_owned().with_id(id)))
    } else {
        if !gist.uri.has_owner() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData, format!("Invalid GitHub gist: {}", gist.uri)));
        }
        match api::iter_gists(&gist.uri.owner).find(|g| gist.uri == g.uri) {
            Some(gist) => {
                debug!("Gist {} found on GitHub with ID={}", gist.uri, gist.id.as_ref().unwrap());
                Ok(Cow::Owned(gist))
            },
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData, format!("Gist {} not found", gist.uri))),
        }
    }
}

/// Obtain the gist ID from its binary path.
fn id_from_binary_path<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let path = try!(path.as_ref().canonicalize());

    // Binaries of GitHub gists are expected to be in the form of:
    // ~/gisht/gists/gh/$ID/$NAME. We want the $ID.
    path.parent().and_then(|p| p.file_stem())
        .and_then(|s| s.to_str()).map(String::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound,
            format!("Invalid GitHub gist binary path: {}", path.display())))
}


/// Check whether given gist needs to be updated.
///
/// If the time since last update cannot be determined for whatever reason,
/// the function will assume the update is necessary.
fn needs_update<G: AsRef<Gist>>(gist: G) -> bool {
    let gist = gist.as_ref();
    trace!("Checking if GitHub gist {} requires an update...", gist.uri);

    let last = match last_update_time(&gist) {
        Ok(time) => time,
        Err(err) => {
            warn!("Couldn't retrieve the last update time of gist {} ({}). \
                   Assuming an update is needed.", gist.uri, err);
            return true;
        },
    };

    let now = SystemTime::now();
    match now.duration_since(last) {
        Ok(duration) => duration > *UPDATE_INTERVAL,
        Err(err) => {
            warn!("Last update time of gist {} is in the future ({}s from now). \
                   Assuming an update is needed.", gist.uri, err.duration().as_secs());
            true
        },
    }
}

/// Determine when was the last time a gist has been updated.
fn last_update_time(gist: &Gist) -> io::Result<SystemTime> {
    // Git writes .git/FETCH_HEAD at every pull, so just check its mtime.
    let fetch_head = gist.path().join(".git").join("FETCH_HEAD");
    fs::metadata(&fetch_head).and_then(|m| m.modified())
}

/// Update an already-downloaded gist.
/// Since GitHub gists are Git repositories, this is basically a `git pull`.
fn update_gist<G: AsRef<Gist>>(gist: G) -> io::Result<()> {
    let gist = gist.as_ref();
    let path = gist.path();
    assert!(gist.id.is_some(), "Gist {} has unknown GitHub ID!", gist.uri);
    assert!(path.exists(), "Directory for gist {} doesn't exist!", gist.uri);

    trace!("Updating GitHub gist {}...", gist.uri);
    let reflog_msg = Some("gisht-update");
    if let Err(err) = git::pull(&path, "origin", reflog_msg) {
        match err.code() {
            git2::ErrorCode::Conflict => {
                warn!("Conflict occurred when updating gist {}, rolling back...", gist.uri);
                try!(git::reset_merge(&path));
                debug!("Conflicting update of gist {} successfully aborted", gist.uri);
            },
            git2::ErrorCode::Uncommitted => {
                // This happens if the user has themselves modified the gist
                // and their changes would be overwritten by the merge.
                // There isn't much we can do in such a case,
                // as it would lead to loss of user's modifications.
                error!("Uncommitted changes found to local copy of gist {}", gist.uri);
                return Err(git::to_io_error(err));
            },
            git2::ErrorCode::Unmerged => {
                // This may happen if previous versions of the application
                // (which didn't handle merge conflicts) has left a mess.
                warn!("Previous unfinished Git merge prevented update of gist {}", gist.uri);
                debug!("Attempting to rollback old Git merge of gist {}...", gist.uri);
                try!(git::reset_merge(&path));
                info!("Old Git merge of gist {} successfully aborted", gist.uri);
            },
            _ => return Err(git::to_io_error(err)),
        }
    }

    debug!("GitHub gist {} successfully updated", gist.uri);
    Ok(())
}


/// Clone the gist's repo into the proper directory (which must NOT exist).
/// Given Gist object must have the GitHub ID associated with it.
fn clone_gist<G: AsRef<Gist>>(gist: G) -> io::Result<()> {
    let gist = gist.as_ref();
    assert!(gist.uri.host_id == ID, "Gist {} is not a GitHub gist!", gist.uri);
    assert!(gist.id.is_some(), "Gist {} has unknown GitHub ID!", gist.uri);
    assert!(!gist.path().exists(), "Directory for gist {} already exists!", gist.uri);

    // Check if the Gist has a clone URL already in its metadata.
    // Otherwise, talk to GitHub to obtain the URL that we can clone the gist from
    // as a Git repository.
    let clone_url = match gist.info(Datum::RawUrl).clone() {
        Some(url) => url,
        None => {
            trace!("Need to get clone URL from GitHub for gist {}", gist.uri);
            let info = try!(api::get_gist_info(&gist.id.as_ref().unwrap()));
            let url = match info.find("git_pull_url").and_then(|u| u.as_string()) {
                Some(url) => url.to_owned(),
                None => {
                    error!("Gist info for {} doesn't contain git_pull_url", gist.uri);
                    return Err(io::Error::new(io::ErrorKind::InvalidData,
                        format!("Couldn't retrieve git_pull_url for gist {}", gist.uri)));
                },
            };
            trace!("GitHub gist #{} has a git_pull_url=\"{}\"",
                gist.id.as_ref().unwrap(), url);
            url
        },
    };

    // Create the gist's directory and clone it as a Git repo there.
    debug!("Cloning GitHub gist from {}", clone_url);
    let path = gist.path();
    try!(fs::create_dir_all(&path));
    try!(git::clone(&clone_url, &path));

    // Make sure the gist's executable is, in fact, executable.
    let executable = gist.path().join(&gist.uri.name);
    try!(mark_executable(&executable));
    trace!("Marked gist file as executable: {}", executable.display());

    // Symlink the main/binary file to the binary directory.
    let binary = gist.binary_path();
    if !binary.exists() {
        try!(fs::create_dir_all(binary.parent().unwrap()));
        try!(symlink_file(&executable, &binary));
        trace!("Created symlink to gist executable: {}", binary.display());
    }

    Ok(())
}


// Utility functions

/// Check if given Gist is a GitHub gist. Invoke using try!().
fn ensure_github_gist(gist: &Gist) -> io::Result<()> {
    if gist.uri.host_id != ID {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!(
            "expected a GitHub Gist, but got a '{}' one", gist.uri.host_id)));
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::{HTML_URL, HTML_URL_RE};

    #[test]
    fn html_url_regex() {
        lazy_static! {
            static ref VALID_HTML_URLS: Vec<(/* URL */   String,
                                             /* owner */ Option<&'static str>,
                                             /* ID */    &'static str)> = vec![
                (HTML_URL.to_owned() + "/foo/123456", Some("foo"), "123456"),
                (HTML_URL.to_owned() + "/Xion/67424258", Some("Xion"), "67424258"),
                (HTML_URL.to_owned() + "/octo-cat/1e2f57a365d782dd36538", Some("octo-cat"), "1e2f57a365d782dd36538"),
                (HTML_URL.to_owned() + "/a", None, "a"),
                (HTML_URL.to_owned() + "/a/1", Some("a"), "1"),
                (HTML_URL.to_owned() + "/42", None, "42"),
                (HTML_URL.to_owned() + "/d0f351a97c65679bb911bafe", None, "d0f351a97c65679bb911bafe"),
            ];
            static ref INVALID_HTML_URLS: Vec<String> = vec![
                HTML_URL.to_owned() + "/a/b/c",         // too many path segments
                HTML_URL.to_owned() + "/a/",            // ID must be provided
                HTML_URL.to_owned() + "/11yf",          // ID must be a hex number
                HTML_URL.to_owned() + "//1",            // owner must not be empty
                HTML_URL.to_owned() + "/",              // no owner nor ID
                "http://github.com/Xion/gisht".into(),  // wrong GitHub domain
                "http://example.com/foo/bar".into(),    // wrong domain altogether
                "foobar".into(),                        // not even an URL
            ];
        }
        for &(ref valid_url, owner, id) in &*VALID_HTML_URLS {
            let captures = HTML_URL_RE.captures(valid_url)
                .expect(&format!("Gist HTML URL was incorrectly deemed invalid: {}", valid_url));
            assert_eq!(owner, captures.name("owner"));
            assert_eq!(id, captures.name("id").unwrap());
        }
        for invalid_url in &*INVALID_HTML_URLS {
            assert!(!HTML_URL_RE.is_match(invalid_url),
                "URL was incorrectly deemed a valid gist HTML URL: {}", invalid_url);
        }
    }
}
