//! Module implementing hastebin.com as gist host.

use std::io;

use regex::Regex;
use url::Url;

use gist::{self, Gist};
use hosts::common::Basic;
use super::{FetchMode, Host};


/// hastebin.com host ID.
pub const ID: &'static str = "hb";


#[derive(Debug)]
pub struct Hastebin {
    inner: Basic,
}

impl Hastebin {
    #[inline]
    pub fn new() -> Self {
        // Hastebin URLs include a completely optional "extension",
        // so the actual URLs can be something like http://hastebin.com/geuyfgdf.foo,
        // where ".foo" is optional indicator of the syntax highlighting
        // to use when displaying the gist in the browser.
        //
        // To support this, we need to wrap Basic in a new type and ensure that:
        //
        // * the extension is stripped when resolving a Hastebin URL
        // * it is added back when the URL is rebuilt
        //   (so that the syntax highlighting can be applied to a website
        //    opened via `gisht show hb:ahgfuehg.foo`).
        //
        let inner = Basic::new(ID, "hastebin.com",
                               "https://hastebin.com/raw/${id}",
                               "https://hastebin.com/${id}",
                               Regex::new("[a-z]+").unwrap()).unwrap();
        Hastebin{inner: inner}
    }
}

impl Host for Hastebin {
    fn id(&self) -> &'static str { self.inner.id() }
    fn name(&self) -> &str { self.inner.name() }

    fn fetch_gist(&self, gist: &Gist, mode: FetchMode) -> io::Result<()> {
        self.inner.fetch_gist(gist, mode)
    }

    /// Return the URL to given hastebin.com gist.
    fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        let mut url = try!(self.inner.gist_url(gist));

        // Replace the "stripped" (extension-less) gist ID in the URL
        // with the "full" one that's been saved on gist info by resolve_url().
        if let Some(ref full_id) = gist.info(gist::Datum::Id) {
            let mut url_obj = Url::parse(&url).unwrap();
            url_obj.path_segments_mut().unwrap().pop().push(full_id);
            url = url_obj.to_string()
        }
        Ok(url)
    }

    fn gist_info(&self, gist: &Gist) -> io::Result<Option<gist::Info>> {
        self.inner.gist_info(gist)
    }

    /// Resolve given URL as potentially pointing to a hastebin.com gist.
    fn resolve_url(&self, mut url: &str) -> Option<io::Result<Gist>> {
        let url_obj = try_opt!(Url::parse(url).ok());

        // Remove the optional "extension" from the given URL,
        // turning http://hastebin.com/qwerty.foo into http://hastebin/qwerty.
        // Preserve it for later inclusion in the gist info.
        let mut extension: Option<&str> = None;  // incl. the dot
        let last_path_segment = try_opt!(url_obj.path_segments().and_then(|ps| ps.last()));
        if let Some(dot_idx) = last_path_segment.rfind(".") {
            let ext = &last_path_segment[dot_idx..];
            extension = Some(ext);
            url = url.trim_right_matches(ext);
        }

        // Resolve the URL using the Basic method and include the ID in gist info.
        let mut gist = match self.inner.resolve_url(url) {
            Some(Ok(gist)) => gist,
            other => return other,
        };
        if let Some(ext) = extension {
            let full_id = format!("{}{}", gist.id.as_ref().unwrap(), ext);
            let info_builder = gist.info.clone()
                .map(|i| i.to_builder()).unwrap_or_else(gist::InfoBuilder::new);
            gist.info = Some(info_builder.with(gist::Datum::Id, &full_id).build());
        }

        Some(Ok(gist))
    }
}



#[cfg(test)]
mod tests {
    use super::Hastebin;

    #[test]
    fn html_url_regex() {
        let host = Hastebin::new();
        let html_url: String = host.inner.html_url_origin();

        let valid_html_urls: Vec<(/* URL */ String,
                                  /* ID */ &'static str)> = vec![
            (html_url.clone() + "/abc", "abc"),                // short
            (html_url.clone() + "/abcdefghij", "abcdefghij"),  // long
        ];
        let invalid_html_urls: Vec<String> = vec![
            html_url.clone() + "/a/b/c",            // too many path segments
            html_url.clone() + "/a/",               // trailing slash
            html_url.clone() + "//",                // ID must not be empty
            html_url.clone() + "/",                 // no ID at all
            html_url.clone() + "/43ffg",            // contains digits
            html_url.clone() + "/MfgTf",            // mixed case
            "http://example.com/fhdgfsgok".into(),  // wrong hastebin.com domain
            "foobar".into(),                        // not even an URL
        ];

        let html_url_re = host.inner.html_url_regex();
        for (ref valid_url, id) in valid_html_urls {
            let captures = html_url_re.captures(valid_url)
                .expect(&format!("Paste's HTML URL was incorrectly deemed invalid: {}", valid_url));
            assert_eq!(id, captures.name("id").unwrap());
        }
        for ref invalid_url in invalid_html_urls {
            assert!(!html_url_re.is_match(invalid_url),
                "URL was incorrectly deemed a valid gist HTML URL: {}", invalid_url);
        }
    }
}
