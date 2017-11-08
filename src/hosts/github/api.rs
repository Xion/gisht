//! Module for interacting with GitHub API.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io;

use hyper;
use hyper::client::{Client, Response};
use hyper::header::UserAgent;
use serde_json::Value as Json;
use url::Url;

use ::USER_AGENT;
use ext::hyper::header::Link;
use gist::{self, Datum, Gist};
use util::http_client;
use super::ID;
use super::util::read_json;


/// Base URL for GitHub API requests.
pub const BASE_URL: &'static str = "https://api.github.com";

/// "Owner" of anonymous gists.
/// GitHub makes these URLs equivalent and pointing to the same gist:
/// https://gist.github.com/anonymous/42 and https://gist.github.com/42
const ANONYMOUS: &'static str = "anonymous";

/// Size of the GitHub response page in items (e.g. gists).
const RESPONSE_PAGE_SIZE: usize = 50;


// Iterating over gists

/// Iterate over GitHub gists belonging to given owner.
#[inline]
pub fn iter_gists(owner: &str) -> GistsIterator {
    // TODO: when `impl Trait` is available, GistsIterator no longer has to be public
    GistsIterator::new(owner)
}


/// Iterator over gists belonging to a particular owner.
#[derive(Debug)]
pub struct GistsIterator<'o> {
    owner: &'o str,
    // Iteration state.
    gists_url: Option<String>,
    gists_json_array: Option<Vec<Json>>,
    index: usize,  // within the above array
    // Other.
    http: Client,
}

impl<'o> GistsIterator<'o> {
    fn new(owner: &'o str) -> Self {
        let gists_url = {
            let mut url = Url::parse(BASE_URL).unwrap();
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
            http: http_client(),
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
            if let Err(error) = self.try_fetch_gists() {
                warn!("Error listing {}'s GitHub gists: {}", self.owner, error);
                return None;
            }
        }

        // Try once more. If we don't get a gist this time, it means we're done.
        self.next_cached()
    }
}

impl<'o> GistsIterator<'o> {
    /// Retrieve the next Gist from a JSON response that's been received previously.
    fn next_cached(&mut self) -> Option<Gist> {
        {
            let gists = try_opt!(self.gists_json_array.as_ref());
            let mut index = self.index;
            while index < gists.len() {
                if let Some(gist) = self.gist_from_json(&gists[index]) {
                    self.index = index + 1;
                    return Some(gist);
                }
                index += 1;
            }
        }
        self.gists_json_array = None;
        self.index = 0;
        None
    }

    /// Try to fetch the next page of gists from GitHub API.
    fn try_fetch_gists(&mut self) -> hyper::Result<()> {
        assert!(self.gists_json_array.is_none());
        assert_eq!(0, self.index);

        let gists_url = self.gists_url.clone().unwrap();
        trace!("Listing GitHub gists from {}", gists_url);

        let mut resp = try!(self.http.get(&*gists_url)
            .header(UserAgent(USER_AGENT.clone()))
            .send());

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
        if let Some(ref link) = resp.headers.get::<Link>() {
            if let Some(next) = link.url("next") {
                self.gists_url = Some(next.into());
                return Ok(());
            }
        }

        debug!("Got to the end of gists for GitHub user {}", self.owner);
        self.gists_url = None;
        Ok(())
    }

    /// Convert a JSON representation of the gist into a Gist object.
    fn gist_from_json(&self, gist: &Json) -> Option<Gist> {
        let id = gist.pointer("/id").and_then(Json::as_str).unwrap();
        let name = match gist_name_from_info(&gist) {
            Some(name) => name,
            None => {
                warn!("GitHub gist #{} (owner={}) has no files", id, self.owner);
                return None;
            },
        };

        let uri = gist::Uri::new(ID, self.owner, name).unwrap();
        if uri.owner != self.owner {
            warn!("Foreign gist ({}, ID={}) found when iterating {}'s gists",
                uri, id, self.owner);
            return None;
        }
        trace!("GitHub gist found ({}) with ID={}", uri, id);

        // Include the gist Info with fields that are commonly used by gist commands.
        // TODO: determine the complete set of fields that can be fetched here
        let info = build_gist_info(&gist, &[Datum::RawUrl, Datum::BrowserUrl]);
        let result = Gist::new(uri, id).with_info(info);
        Some(result)
    }
}


// Fetching gist info

/// Retrieve information/metadata about a gist.
/// Returns a Json object with the parsed GitHub response.
pub fn get_gist_info(gist_id: &str) -> io::Result<Json> {
    let mut gist_url = Url::parse(BASE_URL).unwrap();
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
            match info.find(field).and_then(Json::as_str) {
                Some(value) => { result.set(datum, value); },
                None => { warn!("Missing info key '{}' in gist JSON", field); },
            }
        } else {
            // Special-cased data that are more complicated to get.
            match datum {
                Datum::Owner => { result.set(datum, gist_owner_from_info(&info)); },
                Datum::Language => {
                    match gist_language_from_info(&info) {
                        Some(lang) => { result.set(datum, &*lang); },
                        None => { trace!("Couldn't retrieve the language of GitHub gist"); },
                    }
                },
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
    let filenames = try_opt!(gist_filenames_from_info(info));
    Some(filenames[0])
}

/// Retrieve gist owner from the parsed JSON of gist info.
/// This may be an anonymous name.
pub fn gist_owner_from_info(info: &Json) -> &str {
    info.find_path(&["owner", "login"]).and_then(Json::as_str).unwrap_or(ANONYMOUS)
}

/// Retrieve gist language, if known, from the parsed JSON of gist info.
pub fn gist_language_from_info(info: &Json) -> Option<Cow<str>> {
    let filenames = try_opt!(gist_filenames_from_info(info));
    let get_lang = |filename| {
        info.find_path(&["files", filename, "language"])
            .and_then(Json::as_str).map(|s| s as &str)
    };

    // If there is just one file, its language is the language of the gist.
    if filenames.len() == 1 {
        let language = try_opt!(get_lang(filenames[0]));
        return Some(Cow::Borrowed(language));
    }

    // Otherwise, include the languages of all files, in order, w/o repetitions.
    // (This makes the language of the gist's namesake to appear first).
    let mut languages = Vec::with_capacity(filenames.len());
    let mut langset: HashSet<&str> = HashSet::new();
    for filename in filenames {
        if let Some(lang) = get_lang(filename) {
            if !langset.contains(lang) {
                languages.push(lang);
                langset.insert(lang);
            }
        }
    }
    let result = languages.join(", ").to_string();
    Some(Cow::Owned(result))
}

/// Retrieve the names of the files a gist consists of from the parsed JSON of gist info.
/// The names are sorted alphabetically.
fn gist_filenames_from_info(info: &Json) -> Option<Vec<&str>> {
    let files = try_opt!(info.find("files").and_then(|fs| fs.as_object()));
    let mut filenames: Vec<_> = files.keys().map(|s| s as &str).collect();
     if filenames.is_empty() {
        None
    } else {
        filenames.sort();
        Some(filenames)
    }
}


// Utility functions

/// Make a simple GET request to GitHub API.
fn simple_get(url: Url) -> io::Result<Response> {
    let url = url.into_string();
    let http = http_client();
    http.get(&url)
        .header(UserAgent(USER_AGENT.clone()))
        .send()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}


#[cfg(test)]
mod test {
    use std::borrow::Cow;
    use std::str::FromStr;
    use serde_json::Value as Json;
    use util::http_client;
    use super::{GistsIterator, gist_language_from_info};

    const OWNER: &'static str = "Octocat";
    const GIST_ID: &'static str = "12345";
    const GIST_NAME: &'static str = "test-gist";

    #[test]
    fn gists_iterator_without_items() {
        let mut iter = GistsIterator {
            owner: OWNER,
            gists_url: None,
            gists_json_array: Some(vec![]),
            index: 0,
            http: http_client(),
        };
        assert_eq!(None, iter.next());
    }

    #[test]
    fn gists_iterator_with_cached_items() {
        let gist_json = format!(r#"{{
            "id": "{}",
            "description": "Test gist",
            "owner": {{"login": "{owner}"}},
            "files": {{"{name}": "<omitted>"}}
        }}"#, id=GIST_ID, owner=OWNER, name=GIST_NAME);

        let mut iter = GistsIterator {
            owner: OWNER,
            gists_url: None,
            gists_json_array: Some(vec![Json::from_str(&gist_json).unwrap()]),
            index: 0,
            http: http_client(),
        };
        let gist = iter.next().unwrap();

        assert_eq!(GIST_ID, gist.id.as_ref().unwrap());
        assert_eq!(OWNER, gist.uri.owner);
        assert_eq!(GIST_NAME, gist.uri.name);
    }

    // TODO: test GistsIterator with a mock/fake http_client

    #[test]
    fn gist_language_single() {
        let language = "TrumpScript";
        let gist_json = format!(r#"{{
            "id": "{}",
            "description": "Test gist",
            "owner": {{"login": "{owner}"}},
            "files": {{
                "{name}": {{
                    "language": "{language}"
                }}
            }}
        }}"#, id=GIST_ID, owner=OWNER, name=GIST_NAME, language=language);

        let gist_info = Json::from_str(&gist_json).unwrap();
        let gist_lang = gist_language_from_info(&gist_info).map(Cow::into_owned);
        assert_eq!(Some(language.into()), gist_lang);
    }

    #[test]
    fn gist_language_multiple() {
        let languages = ["TrumpScript", "Haskal", "Rust++"];
        let gist_json = format!(r#"{{
            "id": "{}",
            "description": "Test gist",
            "owner": {{"login": "{owner}"}},
            "files": {{
                "{name}": {{
                    "language": "{lang0}"
                }},
                "second": {{
                    "language": "{lang1}"
                }},
                "third": {{
                    "language": "{lang2}"
                }}
            }}
        }}"#,
        id=GIST_ID, owner=OWNER, name=GIST_NAME,
        lang0=languages[0], lang1=languages[1], lang2=languages[2]);

        let gist_info = Json::from_str(&gist_json).unwrap();
        let gist_lang = gist_language_from_info(&gist_info).map(Cow::into_owned);

        // Note that the exact ordering of languages depends on how the JSON
        // gets parsed into a HashMap, and the resulting order of "files".
        // This is wonkiness inside the whole GitHub gist support code
        // and we cannot really do anything about it :(
        assert!(gist_lang.is_some());
        let expected = {
            let mut langs: Vec<_> = languages.iter().map(ToOwned::to_owned).collect();
            langs.sort();
            langs
        };
        let actual = {
            let mut langs: Vec<_> = gist_lang.unwrap()
                .split(", ").map(ToOwned::to_owned).collect();
            langs.sort();
            langs
        };
        assert_eq!(expected, actual);
    }
}
