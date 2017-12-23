//! Module implementing glot.io as gist host.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Cursor};

use hyper::client::Response;
use hyper::header::UserAgent;
use regex::Regex;
use serde_json::Value as Json;

use ::USER_AGENT;
use gist::{self, Datum, Gist};
use util::{http_client, mark_executable, symlink_file, read_json};
use super::{FetchMode, Host};
use super::common::util::ID_PLACEHOLDER;
use super::common::util::snippet_handler::SnippetHandler;


/// glot.io host ID.
pub const ID: &'static str = "gl";
/// glot.io host name.
const NAME: &'static str = "glot.io";


/// glot.io gist host.
///
/// This host is a little peculiar because it lies somewhere in between
/// the `Basic` hosts and the full-blown, Git-based, complicated host
/// like GitHub.
///
/// Its notable characteristics include:
/// * gists that consist of multiple files, each one named & ordered
///   (based on the order its API returns)
/// * gists that are posted once and not updated (TODO: verify that)
///
#[derive(Debug)]
pub struct Glot {
    /// Snippet handler that's used internally for working with URLs.
    /// Note that:
    /// * "snippets" in this meaning are different that what glot.io itself
    ///   calls snippets (which are just gists like everything else)
    /// * store_gist() isn't used and can't be, because glot.io gists
    ///   consist of multiple files
    handler: SnippetHandler,
    // TODO: consider splitting SnippetHandler into something like
    // SnippetUrlHandler and SnippetStorageHandler, using just the former here
}

impl Glot {
    #[inline]
    pub fn new() -> Self {
        let handler = SnippetHandler::new(
            ID, NAME, HTML_URL_PATTERN,
            Regex::new("[0-9a-z]+").unwrap()).unwrap();
        Glot{handler}
    }
}

impl Host for Glot {
    fn id(&self) -> &'static str { ID }
    fn name(&self) -> &str { NAME }

    fn fetch_gist(&self, gist: &Gist, mode: FetchMode) -> io::Result<()> {
        self.handler.ensure_host_id(gist)?;
        let gist = self.handler.resolve_gist(gist);

        // TODO: it isn't clear if the gists can actually be updated;
        // they do have an "updated" field though so it may be possible for
        // authenticated users; if so, we should support that, too
        // (just overwrite the entire gist directory on each update)
        if self.handler.need_fetch(&*gist, mode)? {
            download_gist(&*gist)?;
        }
        Ok(())
    }

    /// Return the URL to gist's HTML website.
    fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        self.handler.gist_url(gist)
    }

    /// Return a structure with gist metadata.
    fn gist_info(&self, gist: &Gist) -> io::Result<Option<gist::Info>> {
        self.handler.ensure_host_id(gist)?;
        let gist = self.handler.resolve_gist(gist);

        let id = gist.id.as_ref().unwrap();
        let json = api_get_snippet(id)?;

        let result = build_gist_info(&json, &[]);
        Ok(Some(result))
    }

    /// Return a Gist based on URL to its browser HTML page.
    fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
        self.handler.resolve_url(url)
    }
}


/// Pattern of HTML URLs, including the `ID_PLACEHOLDER`.
const HTML_URL_PATTERN: &'static str = "https://glot.io/snippets/${id}";

/// Pattern of API URLs to particular gists, including the `ID_PLACEHOLDER`.
const API_URL_PATTERN: &'static str = "https://snippets.glot.io/snippets/${id}";


/// Download given glot.io gist.
fn download_gist(gist: &Gist) -> io::Result<()> {
    assert!(gist.uri.host_id == ID, "Gist {} is not a {} gist!", gist.uri, NAME);
    assert!(gist.id.is_some(), "Gist {} has unknown {} ID!", gist.uri, NAME);
    assert!(!gist.path().exists(), "Directory for gist {} already exists!", gist.uri);

    debug!("Downloading {} gist {}", NAME, gist.uri);
    let json = api_get_snippet(gist.id.as_ref().unwrap())?;

    // Put all the files in the gist directory, ensuring that it exists first.
    // TODO: check if the filenames are unique
    let path = gist.path();
    let mut executable = None;
    debug!("Saving gist {} under {}", gist.uri, path.display());
    fs::create_dir_all(&path)?;
    let files = json.find("files").and_then(Json::as_array)
        .map(|v| &v[..]).unwrap_or(&[]);
    for (i, file) in files.iter().enumerate() {
        let name = file.find("name").and_then(Json::as_str);
        let content = file.find("content").and_then(Json::as_str).unwrap_or("");
        if name.is_none() {
            error!("Invalid response from glot.io: files/{} has no name", i);
            continue;
        }
        let name = name.unwrap();

        let filepath = path.join(name);
        trace!("Writing file {} of gist {} as {}", name, gist.uri, filepath.display());
        let mut file = fs::OpenOptions::new()
            .create(true).write(true).truncate(true)
            .open(&filepath)?;

        let mut cursor = Cursor::new(content);
        let byte_count = io::copy(&mut cursor, &mut file)?;
        if byte_count == 0 {
            warn!("File {} of gist {} had zero bytes", name, gist.uri);
        } else {
            trace!("Wrote {} byte(s) to {}", byte_count, filepath.display());
        }

        // Additionally, treat the first file as gist's executable.
        // This is speculative, of course,
        // but consistent with the way we're GitHUb gists, for example.
        if i == 0 {
            executable = Some(path.join(&name));
        }
    }

    // Deal with the gist's executable so that it's correctly symlinked.
    if let Some(executable) = executable {
        mark_executable(&executable)?;
        trace!("Marked gist file as executable: {}", executable.display());

        // Create a symlink in the binary directory.
        let binary = gist.binary_path();
        if !binary.exists() {
            try!(fs::create_dir_all(binary.parent().unwrap()));
            try!(symlink_file(&executable, &binary));
            trace!("Created symlink to gist executable: {}", binary.display());
        }
    } else {
        warn!("Gist {} is completely empty (has no files)", gist.uri);
    }

    Ok(())
}


// Fetching gist info

/// Retrieve gist from glot.io API (which is called a "snippet" there).
/// JSON response is described here:
/// https://github.com/prasmussen/glot-snippets/blob/master/api_docs/get_snippet.md.
fn api_get_snippet(id: &str) -> io::Result<Json> {
    debug!("Getting glot.io snippet with ID={}", id);
    let url = API_URL_PATTERN.replace(ID_PLACEHOLDER, id);
    trace!("Sending GET to {}", url);
    let mut resp = simple_get(url)?;
    read_json(&mut resp)
}

/// GIven a JSON response from glot.io "Get snippet" request,
/// build a gist Info structure.
/// If `data` is non-empty, only selected fields are included in the info.
fn build_gist_info(json: &Json, data: &[Datum]) -> gist::Info {
    let mut data: Vec<_> = data.to_vec();
    if data.is_empty() {
        data = Datum::iter_variants().collect();
    }

    lazy_static! {
        // Mapping of gist::Info items to keys in the JSON.
        static ref INFO_FIELDS: HashMap<Datum, &'static str> = hashmap!{
            Datum::Owner => "owner",
            Datum::Description => "title", // close enough
            Datum::Language => "language",
            Datum::RawUrl => "url",
            Datum::CreatedAt => "created",
            Datum::UpdatedAt => "modified",
        };
    }

    // Extract ID separately since it's used for creating BrowserUrl too.
    let id = json.find("id").and_then(Json::as_str).unwrap_or_else(|| {
        panic!("'id' not found in glot.io JSON response!");
    });

    let mut result = gist::InfoBuilder::new();
    for datum in data {
        if let Some(field) = INFO_FIELDS.get(&datum) {
            match json.find(field).and_then(Json::as_str) {
                Some(value) => { result.set(datum, value); },
                None => { warn!("Missing info key '{}' in gist JSON", field); },
            }
        } else {
            // Special-cased data that are more complicated to get.
            match datum {
                Datum::Id => { result.set(datum, id); }
                Datum::BrowserUrl => {
                    let url = HTML_URL_PATTERN.replace(ID_PLACEHOLDER, id);
                    result.set(datum, &url);
                }
                _ => {
                    panic!("Unexpected {} gist info data piece: {:?}", NAME, datum);
                }
            }
        }
    }
    result.build()
}


// Utility functions

/// Make a simple GET request to GitHub API.
fn simple_get<U: ToString>(url: U) -> io::Result<Response> {
    let url = url.to_string();
    let http = http_client();
    http.get(&url)
        .header(UserAgent(USER_AGENT.clone()))
        .send()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}


#[cfg(test)]
mod tests {
    use super::{API_URL_PATTERN, HTML_URL_PATTERN, ID_PLACEHOLDER};

    #[test]
    fn valid_html_url_pattern() {
        assert!(HTML_URL_PATTERN.contains(ID_PLACEHOLDER));
    }

    #[test]
    fn valid_api_url_pattern() {
        assert!(API_URL_PATTERN.contains(ID_PLACEHOLDER));
    }
}
