//! Module implementing hastebin.com as gist host.

use std::io;

use regex::Regex;

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
        // TODO: In reality, the URLs seem to include a completely optional "extension",
        // so the actual URLs can be something like http://hastebin.com/geuyfgdf.foo,
        // where ".foo" is optional indicator of the syntax highlighting
        // to use when displaying the gist in the browser.
        //
        // To support that, we may need to wrap Basic in a new type.
        // For maximum functionality, we'd also have to recreate the original "extension",
        // so that the syntax highlighting can be applied to a website opened via
        // `gisht show hb:ahgfuehg.foo`.
        //
        // Alternatively, just store the extension as part of the gist ID.
        // The downside is potentially having multiple copies of the same gist,
        // under abcdef.foo and abcdef.bar.

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

    fn gist_url(&self, gist: &Gist) -> io::Result<String> {
        self.inner.gist_url(gist)
    }

    fn gist_info(&self, gist: &Gist) -> io::Result<Option<gist::Info>> {
        self.inner.gist_info(gist)
    }

    fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
        self.inner.resolve_url(url)
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
