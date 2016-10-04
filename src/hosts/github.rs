//! Module implementing GitHub as gist host.
//!
//! This is specifically about the gist.github.com part of GitHub,
//! NOT the actual GitHub repository hosting.

use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::marker::PhantomData;
use std::path::Path;

use git2::{self, Repository};
use hyper::client::{Client, Response};
use hyper::header::{ContentLength, UserAgent};
use rustc_serialize::json::Json;
use url::Url;

use super::super::USER_AGENT;
use ext::hyper::header::Link;
use gist::{self, Datum, Gist};
use util::{mark_executable, symlink_file};
use super::Host;


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
    fn fetch_gist(&self, gist: &Gist) -> io::Result<()> {
        try!(ensure_github_gist(gist));
        let gist = try!(resolve_gist(gist));

        if gist.is_local() {
            try!(update_gist(gist));
        } else {
            try!(clone_gist(gist));
        }

        Ok(())
    }

    /// Return the URL to gist's HTML website.
    fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        // TODO: get the URL from GitHub directly ('html_url' field of gist info)
        // rather than formatting it manually
        let gist = try!(resolve_gist(gist));
        let mut url = Url::parse(HTML_URL).unwrap();
        url.set_path(&format!("{}/{}", gist.uri.owner, gist.id.as_ref().unwrap()));
        Ok(url.into_string())
    }

    /// Return a structure with gist metadata.
    fn gist_info(&self, gist: &Gist) -> io::Result<Option<gist::Info>> {
        try!(ensure_github_gist(gist));
        let gist = try!(resolve_gist(gist));

        let info = try!(get_gist_info(gist.id.as_ref().unwrap()));

        // Build the gist::Info structure from known keys in the gist info JSON.
        const INFO_FIELDS: &'static [(Datum, &'static str)] = &[
            (Datum::Id, "id"),
            (Datum::Description, "description"),
            (Datum::Url, "html_url"),
            (Datum::CreatedAt, "created_at"),
            (Datum::UpdatedAt, "updated_at"),
        ];
        let mut result = gist::InfoBuilder::new();
        for &(datum, field) in INFO_FIELDS {
            result.set(datum, info[field].as_string().unwrap());
        }
        result.set(Datum::Owner, info["owner"]["login"].as_string().unwrap());
        Ok(Some(result.build()))
    }
}

/// Base URL to gist HTML pages.
const HTML_URL: &'static str = "http://gist.github.com";


// Fetching gists

/// Base URL for GitHub API requests.
const API_URL: &'static str = "https://api.github.com";

/// Size of the GitHub response page in items (e.g. gists).
const RESPONSE_PAGE_SIZE: usize = 50;


/// Return a "resolved" Gist that has a GitHub ID associated with it.
fn resolve_gist(gist: &Gist) -> io::Result<Cow<Gist>> {
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
        let gists = list_gists(&gist.uri.owner);
        match gists.into_iter().find(|g| gist.uri == g.uri) {
            Some(gist) => Ok(Cow::Owned(gist)),
            _ => return Err(io::Error::new(
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

    // Talk to GitHub to obtain the URL that we can clone the gist from
    // as a Git repository.
    let clone_url = {
        let gist_info = try!(get_gist_info(&gist.id.as_ref().unwrap()));
        let clone_url = gist_info["git_pull_url"].as_string().unwrap().to_owned();
        trace!("GitHub gist #{} has a git_pull_url=\"{}\"",
            gist_info["id"].as_string().unwrap(), clone_url);
        clone_url
    };

    // Create the gist's directory and clone it as a Git repo there.
    let path = gist.path();
    try!(fs::create_dir_all(&path));
    try!(Repository::clone(&clone_url, &path)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

    // Make sure the gist's executable is, in fact, executable.
    let executable = gist.path().join(&gist.uri.name);
    try!(mark_executable(&executable));

    // Symlink the main/binary file to the binary directory.
    let binary = gist.binary_path();
    if !binary.exists() {
        try!(fs::create_dir_all(binary.parent().unwrap()));
        try!(symlink_file(&executable, &binary));
    }

    Ok(())
}


/// Get all GitHub gists belonging to a given owner.
fn list_gists(owner: &str) -> Vec<Gist> {
    let http = Client::new();

    let mut gists_url = {
        let mut url = Url::parse(API_URL).unwrap();
        url.set_path(&format!("users/{}/gists", owner));
        url.query_pairs_mut()
            .append_pair("per_page", &RESPONSE_PAGE_SIZE.to_string());
        url.into_string()
    };

    let mut result = HashSet::new();
    loop {
        debug!("Listing GitHub gists from {}", gists_url);
        let mut resp = http.get(&gists_url)
            .header(UserAgent(USER_AGENT.clone()))
            .send().unwrap();

        // Parse the response as JSON array and extract gist names from it.
        let gists_json = read_json(&mut resp);
        if let Json::Array(gists) = gists_json {
            trace!("Result page with {} gist(s) found", gists.len());
            for gist in gists {
                let id = gist["id"].as_string().unwrap();

                // GitHub names gists after first files in alphabetical order,
                // so we need to find that first file.
                let mut gist_files: Vec<_> = gist["files"].as_object().unwrap()
                    .keys().collect();
                if gist_files.is_empty() {
                    warn!("GitHub gist #{} (owner={}) has no files", id, owner);
                    continue;
                }
                gist_files.sort();
                let gist_name = gist_files[0];

                let gist_uri = gist::Uri::new("gh", owner, gist_name).unwrap();
                trace!("GitHub gist found ({}) with id={}", gist_uri, id);
                if !result.insert(Gist::new(gist_uri, id)) {
                    // TODO: find a way to warn the user about this ambiguity
                    warn!("GitHub gist {}/{} is a duplicate, skipping.", owner, gist_name);
                }
            }
        } else {
            warn!("Invalid JSON format of GitHub gist result page ({})", gists_url);
        }

        // Determine the URL to get the next page of gists from.
        if let Some(&Link(ref links)) = resp.headers.get::<Link>() {
            if let Some(next) = links.get("next") {
                gists_url = next.url.clone();
                continue;
            }
        }

        debug!("{} gist(s) found in total", result.len());
        break;
    }
    result.into_iter().collect()
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


// Utility functions

/// Check if given Gist is a GitHub gist. Invoke using try!().
fn ensure_github_gist(gist: &Gist) -> io::Result<()> {
    if gist.uri.host_id != "gh" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!(
            "expected a GitHub Gist, but got a '{}' one", gist.uri.host_id)));
    }
    Ok(())
}

/// Read HTTP response from hype and parse it as JSON.
fn read_json(response: &mut Response) -> Json {
    let mut body = match response.headers.get::<ContentLength>() {
        Some(&ContentLength(l)) => String::with_capacity(l as usize),
        _ => String::new(),
    };
    response.read_to_string(&mut body).unwrap();
    Json::from_str(&body).unwrap()
}
