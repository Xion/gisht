//! MOdule for interacting with GitHub API.

use std::collections::HashMap;
use std::io;

use hyper::client::{Client, Response};
use hyper::header::UserAgent;
use rustc_serialize::json::Json;
use url::Url;

use ::USER_AGENT;
use gist::{self, Datum};
use super::util::read_json;


/// Base URL for GitHub API requests.
pub const API_URL: &'static str = "https://api.github.com";

/// "Owner" of anonymous gists.
/// GitHub makes these URLs equivalent and pointing to the same gist:
/// https://gist.github.com/anonymous/42 and https://gist.github.com/42
const ANONYMOUS: &'static str = "anonymous";


// Fetching gist info

/// Retrieve information/metadata about a gist.
/// Returns a Json object with the parsed GitHub response.
pub fn get_gist_info(gist_id: &str) -> io::Result<Json> {
    let mut gist_url = Url::parse(API_URL).unwrap();
    gist_url.set_path(&format!("gists/{}", gist_id));

    debug!("Getting GitHub gist info from {}", gist_url);
    let mut resp = try!(simple_get(gist_url));
    Ok(read_json(&mut resp))
}

/// Build the complete gist Info from its GitHub JSON representation.
/// If fields are non-empty, only selected fields are included in the info.
pub fn build_gist_info(info: &Json, data: &[Datum]) -> gist::Info {
    let mut data: Vec<_> = data.to_vec();
    if data.is_empty() {
        data = Datum::iter_variants().collect();
    }

    lazy_static! {
        // Mapping of gist::Info items to keys in the JSON.
        static ref INFO_FIELDS: HashMap<Datum, &'static str> = hashmap!{
            Datum::Id => "id",
            Datum::Description => "description",
            Datum::BrowserUrl => "html_url",
            Datum::RawUrl => "git_pull_url",
            Datum::CreatedAt => "created_at",
            Datum::UpdatedAt => "updated_at",
        };
    }
    let mut result = gist::InfoBuilder::new();
    for datum in data {
        if let Some(field) = INFO_FIELDS.get(&datum) {
            match info.find(field).and_then(|f| f.as_string()) {
                Some(value) => { result.set(datum, value); },
                None => { warn!("Missing info key '{}' in gist JSON", field); },
            }
        } else {
            // Special-cased data that are more complicated to get.
            match datum {
                Datum::Owner => { result.set(datum, gist_owner_from_info(&info)); },
                _ => { panic!("Unexpected gist info data piece: {:?}", datum); },
            }
        }
    }
    result.build()
}


// Handling gist info JSON

/// Retrieve gist name from the parsed JSON of gist info.
///
/// The gist name is defined to be the name of its first file,
/// as this is how GitHub page itself picks it.
pub fn gist_name_from_info(info: &Json) -> Option<&str> {
    let files = try_opt!(info.find("files").and_then(|fs| fs.as_object()));
    let mut filenames: Vec<_> = files.keys().map(|s| s as &str).collect();
    if filenames.is_empty() {
        None
    } else {
        filenames.sort();
        Some(filenames[0])
    }
}

/// Retrieve gist owner from the parsed JSON of gist info.
/// This may be an anonymous name.
pub fn gist_owner_from_info(info: &Json) -> &str {
    info.find_path(&["owner", "login"])
        .and_then(|l| l.as_string())
        .unwrap_or(ANONYMOUS)
}


// Utility functions

/// Make a simple GET request to GitHub API.
fn simple_get(url: Url) -> io::Result<Response> {
    let url = url.into_string();
    let http = Client::new();
    http.get(&url)
        .header(UserAgent(USER_AGENT.clone()))
        .send()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}
