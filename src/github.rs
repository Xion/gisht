//! Module implementing GitHub as gist host.

use std::io::Read;
use std::marker::PhantomData;

use hyper::client::{Client, Response};
use hyper::header::{ContentLength, UserAgent};
use rustc_serialize::json::Json;
use url::Url;

use gist;
use util::header::Link;
use super::USER_AGENT;


#[derive(Debug)]
pub struct GitHub {
    _marker: PhantomData<()>,
}

impl GitHub {
    pub fn new() -> Self {
        GitHub { _marker: PhantomData }
    }
}

impl gist::Host for GitHub {
    fn name(&self) -> &str { "GitHub" }

    fn gists(&self, owner: &str) -> Vec<gist::Uri> {
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
                    // GitHub names gists after first files in alphabetical order,
                    // so we need to find that first file.
                    // TODO: warn the user when this could create ambiguity,
                    // with two gists named the same way according to this scheme
                    let mut gist_files: Vec<_> = gist["files"].as_object().unwrap()
                        .keys().collect();
                    if gist_files.is_empty() {
                        warn!("GitHub gist #{} (owner={}) has no files",
                            gist["id"].as_string().unwrap(), owner);
                        continue;
                    }
                    gist_files.sort();
                    let gist_name = gist_files[0];

                    let gist_uri = gist::Uri::new("gh", owner, gist_name).unwrap();
                    trace!("GitHub gist found ({})", gist_uri);
                    result.push(gist_uri);
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
}

const GIST_BASE_URL: &'static str = "http://gist.github.com";

const API_URL: &'static str = "https://api.github.com";
/// Size of the GitHub response page in items (e.g. gists).
const RESPONSE_PAGE_SIZE: usize = 50;


// Utility functions

fn read_json(response: &mut Response) -> Json {
    let mut body = match response.headers.get::<ContentLength>() {
        Some(&ContentLength(l)) => String::with_capacity(l as usize),
        _ => String::new(),
    };
    response.read_to_string(&mut body).unwrap();
    Json::from_str(&body).unwrap()
}
