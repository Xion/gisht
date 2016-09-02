//! Module implementing GitHub as gist host.
//!
//! This is specifically about the gist.github.com part of GitHub,
//! NOT the actual GitHub repository hosting.

use std::borrow::Cow;
use std::fs;
use std::io::{self, Read};
use std::marker::PhantomData;
use std::path::Path;

use git2::Repository;
use hyper::client::{Client, Response};
use hyper::header::{ContentLength, UserAgent};
use rustc_serialize::json::Json;
use url::Url;

use super::super::USER_AGENT;
use ext::hyper::header::Link;
use gist::{self, Gist};
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

    /// Get all GitHub gists belonging to given owner.
    fn gists(&self, owner: &str) -> Vec<Gist> {
        list_gists(owner)
    }

    /// Download the GitHub gist's repo & create the appropriate binary symlink.
    ///
    /// If the gist hasn't been downloaded already, a clone of the gist's Git repo is performed.
    /// Otherwise, it's just a simple Git pull.
    fn download_gist(&self, gist: &Gist) -> io::Result<()> {
        if gist.uri.host_id != "gh" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!(
                "expected a GitHub Gist, but got a '{}' one", gist.uri.host_id)));
        }

        // If the gist doesn't have the ID associated with it,
        // resolve the owner/name by either checking the already existing,
        // local gist, or listing all the owner's gists to find the matching ID.
        let mut gist = Cow::Borrowed(gist);
        if !gist.id.is_some() {
            // TODO: if the gist's repo already exists, simply perform a git pull
            // instead polling for gist info and cloning the repo
            gist = if gist.is_local() {
                let id = try!(id_from_binary_path(gist.binary_path()));
                Cow::Owned(gist.into_owned().with_id(id))
            } else {
                let gists = list_gists(&gist.uri.owner);
                match gists.into_iter().find(|g| gist.uri == g.uri) {
                    Some(gist) => Cow::Owned(gist),
                    _ => return Err(io::Error::new(
                        io::ErrorKind::InvalidData, format!("Gist {} not found", gist.uri))),
                }
            }
        }

        // Talk to GitHub to obtain the URL that we can clone the gist from
        // as a Git repository.
        let clone_url = {
            let http = Client::new();

            let gist_url = {
                let mut url = Url::parse(API_URL).unwrap();
                url.set_path(&format!("gists/{}", gist.id.as_ref().unwrap()));
                url.into_string()
            };

            debug!("Getting GitHub gist info from {}", gist_url);
            let mut resp = try!(http.get(&gist_url)
                .header(UserAgent(USER_AGENT.clone()))
                .send()
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));
            let gist_info_json = read_json(&mut resp);

            let clone_url = gist_info_json["git_pull_url"].as_string().unwrap().to_owned();
            trace!("GitHub gist #{} has a git_pull_url=\"{}\"",
                gist_info_json["id"].as_string().unwrap(), clone_url);
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
}

/// Base URL to gist HTML pages.
const HTML_URL: &'static str = "http://gist.github.com";

/// Base URL for GitHub API requests.
const API_URL: &'static str = "https://api.github.com";
/// Size of the GitHub response page in items (e.g. gists).
const RESPONSE_PAGE_SIZE: usize = 50;


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

    let mut result = Vec::new();
    loop {
        debug!("Listing GitHub gists from {}", gists_url);
        let mut resp = http.get(&gists_url)
            .header(UserAgent(USER_AGENT.clone()))
            .send().unwrap();

        // Parse the response as JSON array and extract gist names from it.
        let gists_json = read_json(&mut resp);
        if let Json::Array(gists) = gists_json {
            debug!("{} gist(s) found", gists.len());
            for gist in gists {
                let id = gist["id"].as_string().unwrap();

                // GitHub names gists after first files in alphabetical order,
                // so we need to find that first file.
                // TODO: warn the user when this could create ambiguity,
                // with two gists named the same way according to this scheme
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
                result.push(Gist::new(gist_uri, id));
            }
        }

        // Determine the URL to get the next page of gists from.
        if let Some(&Link(ref links)) = resp.headers.get::<Link>() {
            if let Some(next) = links.get("next") {
                gists_url = next.url.clone();
                continue;
            }
        }
        break;
    }
    result
}


// Utility functions

/// Read HTTP response from hype and parse it as JSON.
fn read_json(response: &mut Response) -> Json {
    let mut body = match response.headers.get::<ContentLength>() {
        Some(&ContentLength(l)) => String::with_capacity(l as usize),
        _ => String::new(),
    };
    response.read_to_string(&mut body).unwrap();
    Json::from_str(&body).unwrap()
}
