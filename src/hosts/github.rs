//! Module implementing GitHub as gist host.
//!
//! This is specifically about the gist.github.com part of GitHub,
//! NOT the actual GitHub repository hosting.

use std::borrow::Cow;
use std::fs;
use std::io::{self, Read};
use std::iter::Iterator;
use std::marker::PhantomData;
use std::path::Path;
use std::time::{Duration, SystemTime};

use git2::{self, Repository};
use hyper::client::{Client, Response};
use hyper::header::{ContentLength, UserAgent};
use regex::{self, Regex};
use rustc_serialize::json::Json;
use url::Url;

use super::super::USER_AGENT;
use ext::hyper::header::Link;
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
    fn name(&self) -> &str { "GitHub" }

    /// Fetch the gist's repo from GitHub & create the appropriate binary symlink.
    ///
    /// If the gist hasn't been downloaded already, a clone of the gist's Git repo is performed.
    /// Otherwise, it's just a simple Git pull.
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

        let url = {
            let mut url = Url::parse(HTML_URL).unwrap();
            url.set_path(&format!("{}/{}", gist.uri.owner, gist.id.as_ref().unwrap()));
            url.into_string()
        };
        trace!("Browser URL for {:?}: {}", gist, url);
        Ok(url)
    }

    /// Return a structure with gist metadata.
    fn gist_info(&self, gist: &Gist) -> io::Result<Option<gist::Info>> {
        try!(ensure_github_gist(gist));
        let gist = try!(resolve_gist(gist));

        let id = gist.id.as_ref().unwrap();
        let info = try!(get_gist_info(id));

        // Build the gist::Info structure from known keys in the gist info JSON.
        const INFO_FIELDS: &'static [(Datum, &'static str)] = &[
            (Datum::Id, "id"),
            (Datum::Description, "description"),
            (Datum::BrowserUrl, "html_url"),
            (Datum::RawUrl, "git_pull_url"),
            (Datum::CreatedAt, "created_at"),
            (Datum::UpdatedAt, "updated_at"),
        ];
        let mut result = gist::InfoBuilder::new();
        for &(datum, field) in INFO_FIELDS {
            match info.find(field).and_then(|f| f.as_string()) {
                Some(value) => { result.set(datum, value); },
                None => { warn!("Missing info key '{}' for gist ID={}", field, id); },
            }
        }
        result.set(Datum::Owner, gist_owner_from_info(&info));
        Ok(Some(result.build()))
    }

    /// Return a Gist based on URL to its URL page.
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
        let info = try_some!(get_gist_info(id));
        let name = match gist_name_from_info(&info) {
            Some(name) => name,
            None => {
                warn!("GitHub gist with ID={} (URL={}) has no files", id, orig_url);
                return None;
            },
        };
        let owner = captures.name("owner").unwrap_or_else(|| gist_owner_from_info(&info));

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
        &format!("^{}{}$", regex::quote(HTML_URL), r#"/((?P<owner>[^/]+)/)?(?P<id>[0-9a-fA-F]+)"#)
    ).unwrap();
}

/// "Owner" of anonymous gists.
/// GitHub makes these URLs equivalent and pointing to the same gist:
/// https://gist.github.com/anonymous/42 and https://gist.github.com/42
const ANONYMOUS: &'static str = "anonymous";


// Fetching gists

/// Base URL for GitHub API requests.
const API_URL: &'static str = "https://api.github.com";

/// Size of the GitHub response page in items (e.g. gists).
const RESPONSE_PAGE_SIZE: usize = 50;

lazy_static! {
    /// Minimum interval between updating (git-pulling) of gists.
    static ref UPDATE_INTERVAL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
}


/// Return a "resolved" Gist that has a GitHub ID associated with it.
fn resolve_gist(gist: &Gist) -> io::Result<Cow<Gist>> {
    debug!("Resolving GitHub gist: {}", gist.uri);
    let gist = Cow::Borrowed(gist);
    if gist.id.is_some() {
        return Ok(gist);
    }

    // If the gist doesn't have the ID associated with it,
    // resolve the owner/name by either checking the already existing,
    // local gist, or listing all the owner's gists to find the matching ID.
    if gist.is_local() {
        let id = try!(id_from_binary_path(gist.binary_path()));
        Ok(Cow::Owned(gist.into_owned().with_id(id)))
    } else {
        if !gist.uri.has_owner() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData, format!("Invalid GitHub gist: {}", gist.uri)));
        }
        match iter_gists(&gist.uri.owner).find(|g| gist.uri == g.uri) {
            Some(gist) => Ok(Cow::Owned(gist)),
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
    assert!(gist.id.is_some(), "Gist {} has unknown GitHub ID!", gist.uri);
    assert!(gist.path().exists(), "Directory for gist {} doesn't exist!", gist.uri);

    try!(git_pull(gist.path(), "origin", /* reflog_msg */ Some("gisht-update"))
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));
    // TODO: conficts?

    Ok(())
}

/// Perform a standard Git "pull" operation.
fn git_pull<P: AsRef<Path>>(repo_path: P,
                            remote: &str,
                            reflog_msg: Option<&str>) -> Result<(), git2::Error> {
    // Since libgit2 is low-level, we have to perform the requisite steps manually,
    // which means:
    // * doing the fetch from origin remote
    // * checking out the (new) HEAD
    let repo = try!(Repository::open(repo_path));
    let mut origin = try!(repo.find_remote(remote));
    try!(origin.fetch(/* refspecs */ &[], /* options */ None, reflog_msg));
    try!(repo.checkout_head(/* options */ None));

    Ok(())
}


/// Clone the gist's repo into the proper directory (which must NOT exist).
/// Given Gist object must have the GitHub ID associated with it.
fn clone_gist<G: AsRef<Gist>>(gist: G) -> io::Result<()> {
    let gist = gist.as_ref();
    assert!(gist.id.is_some(), "Gist {} has unknown GitHub ID!", gist.uri);
    assert!(!gist.path().exists(), "Directory for gist {} already exists!", gist.uri);

    // Check if the Gist has a clone URL already in its metadata.
    // Otherwise, talk to GitHub to obtain the URL that we can clone the gist from
    // as a Git repository.
    let clone_url = match gist.info(Datum::RawUrl).clone() {
        Some(url) => url,
        None => {
            trace!("Need to get clone URL from GitHub for gist {}", gist.uri);
            let info = try!(get_gist_info(&gist.id.as_ref().unwrap()));
            let url = info["git_pull_url"].as_string().unwrap().to_owned();
            trace!("GitHub gist #{} has a git_pull_url=\"{}\"",
                info["id"].as_string().unwrap(), url);
            url
        }
    };

    // Create the gist's directory and clone it as a Git repo there.
    debug!("Cloning GitHub gist from {}", clone_url);
    let path = gist.path();
    try!(fs::create_dir_all(&path));
    try!(Repository::clone(&clone_url, &path)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

    // Make sure the gist's executable is, in fact, executable.
    let executable = gist.path().join(&gist.uri.name);
    try!(mark_executable(&executable));
    trace!("Marked file as gist executable: {}", executable.display());

    // Symlink the main/binary file to the binary directory.
    let binary = gist.binary_path();
    if !binary.exists() {
        try!(fs::create_dir_all(binary.parent().unwrap()));
        try!(symlink_file(&executable, &binary));
        trace!("Created symlink to gist executable: {}", binary.display());
    }

    Ok(())
}


/// Iterate over GitHub gists belonging to given owner.
#[inline]
fn iter_gists(owner: &str) -> GistsIterator {
    GistsIterator::new(owner)
}

/// Iterator over gists belonging to a particular owner.
struct GistsIterator<'o> {
    owner: &'o str,
    // Iteration state.
    gists_url: Option<String>,
    gists_json_array: Option<Vec<Json>>,
    index: usize,  // within the above array
    // Other.
    http: Client,
}
impl<'o> GistsIterator<'o> {
    pub fn new(owner: &'o str) -> Self {
        let gists_url = {
            let mut url = Url::parse(API_URL).unwrap();
            url.set_path(&format!("users/{}/gists", owner));
            url.query_pairs_mut()
                .append_pair("per_page", &RESPONSE_PAGE_SIZE.to_string());
            url.into_string()
        };

        debug!("Iterating over GitHub gists for user {}", owner);
        GistsIterator{
            owner: owner,
            gists_url: Some(gists_url),
            gists_json_array: None,
            index: 0,
            http: Client::new(),
        }
    }
}
impl<'o> Iterator for GistsIterator<'o> {
    type Item = Gist;

    fn next(&mut self) -> Option<Self::Item> {
        // First, try to get the next gist from the cached JSON response, if any.
        if let Some(gist) = self.next_cached() {
            return Some(gist);
        }

        // If we don't have any cached gists in JSON form,
        // talk to the GitHub API to obtain the next (or first) page.
        if self.gists_json_array.is_none() && self.gists_url.is_some() {
            self.try_fetch_gists();
        }

        // Try once more. If we don't get a gist time, it means we're done.
        self.next_cached()
    }
}
impl<'o> GistsIterator<'o> {
    /// Retrieve the next Gist from a JSON response that's been received previously.
    fn next_cached(&mut self) -> Option<Gist> {
        if self.gists_json_array.is_none() {
            return None;
        }

        let gists_len = self.gists_json_array.as_ref().unwrap().len();
        let mut index = self.index;
        while index < gists_len {
            let gists = self.gists_json_array.as_ref().unwrap();
            if let Some(gist) = self.gist_from_json(&gists[index]) {
                self.index = index + 1;
                return Some(gist);
            }
            index += 1;
        }

        self.gists_json_array = None;
        self.index = 0;
        None
    }

    /// Try to fetch the next page of gists from GitHub API.
    fn try_fetch_gists(&mut self) {
        assert!(self.gists_json_array.is_none());
        assert_eq!(0, self.index);

        let gists_url = self.gists_url.clone().unwrap();
        trace!("Listing GitHub gists from {}", gists_url);

        let mut resp = self.http.get(&*gists_url)
            .header(UserAgent(USER_AGENT.clone()))
            .send().unwrap();

        // Parse the response as JSON array and extract gist names from it.
        let gists_json = read_json(&mut resp);
        if let Json::Array(gists) = gists_json {
            let page_size = gists.len();
            self.gists_json_array = Some(gists);
            trace!("Result page with {} gist(s) found", page_size);
        } else {
            warn!("Invalid JSON format of GitHub gist result page ({})", gists_url);
        }

        // Determine the URL to get the next page of gists from.
        if let Some(&Link(ref links)) = resp.headers.get::<Link>() {
            if let Some(next) = links.get("next") {
                self.gists_url = Some(next.url.clone());
                return;
            }
        }

        debug!("Got to the end of gists for GitHub user {}", self.owner);
        self.gists_url = None;
    }

    /// Convert a JSON representation of the gist into a Gist object.
    fn gist_from_json(&self, gist: &Json) -> Option<Gist> {
        let id = gist["id"].as_string().unwrap();
        let name = match gist_name_from_info(&gist) {
            Some(name) => name,
            None => {
                warn!("GitHub gist #{} (owner={}) has no files", id, self.owner);
                return None;
            },
        };
        let uri = gist::Uri::new(ID, self.owner, name).unwrap();
        trace!("GitHub gist found ({}) with id={}", uri, id);

        // TODO: include the complete gist Info here by reusing the code from GitHub::gist_info;
        // right now we only need clone URL so putting only it
        let info = gist::InfoBuilder::new()
            .with_opt(Datum::RawUrl, gist.find("git_pull_url").and_then(|url| url.as_string()))
            .build();
        let gist = Gist::new(uri, id).with_info(info);
        Some(gist)
    }
}


/// Retrieve information/metadata about a gist.
/// Returns a Json object with the parsed GitHub response.
fn get_gist_info(gist_id: &str) -> io::Result<Json> {
    let http = Client::new();

    let gist_url = {
        let mut url = Url::parse(API_URL).unwrap();
        url.set_path(&format!("gists/{}", gist_id));
        url.into_string()
    };

    debug!("Getting GitHub gist info from {}", gist_url);
    let mut resp = try!(http.get(&gist_url)
        .header(UserAgent(USER_AGENT.clone()))
        .send()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));
    Ok(read_json(&mut resp))
}

/// Retrieve gist name from the parsed JSON of gist info.
///
/// The gist name is defined to be the name of its first file,
/// as this is how GitHub page itself picks it.
fn gist_name_from_info(info: &Json) -> Option<&str> {
    info.find("files").and_then(|fs| fs.as_object()).and_then(|files| {
        let mut filenames: Vec<_> = files.keys().map(|s| s as &str).collect();
        if filenames.is_empty() {
            None
        } else {
            filenames.sort();
            Some(filenames[0])
        }
    })
}

/// Retrieve gist owner from the parsed JSON of gist info.
/// This may be an anonymous name.
fn gist_owner_from_info(info: &Json) -> &str {
    info.find_path(&["owner", "login"])
        .and_then(|l| l.as_string())
        .unwrap_or(ANONYMOUS)
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

/// Read HTTP response from hyper and parse it as JSON.
fn read_json(response: &mut Response) -> Json {
    let mut body = match response.headers.get::<ContentLength>() {
        Some(&ContentLength(l)) => String::with_capacity(l as usize),
        _ => String::new(),
    };
    response.read_to_string(&mut body).unwrap();
    Json::from_str(&body).unwrap()
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
