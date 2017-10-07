//! Module implementing mibpaste.com as a gist host.

use regex::Regex;

use hosts::common::Basic;


/// mibpaste.com host ID.
pub const ID: &'static str = "mib";


/// Mibbit paste host.
///
/// This host is somewhat special insofar that it doesn't seem to allow
/// creating pastes independently. The pastes come from the Mibbit
/// web IRC client.
pub type Mibpaste = internal::Mibpaste<Basic>;

impl Mibpaste {
    #[inline]
    pub fn new() -> Self {
        // Mibpaste pastes do not have a separate raw page, offering
        // only the HTML page, so both types of URLs use the same pattern.
        // The HTML page is later stripped to retain only the paste's text.
        let url_pattern = "http://mibpaste.com/${id}";

        let inner = Basic::new(ID, "Mibbit",
                               url_pattern, url_pattern,
                               Regex::new("[0-9a-zA-Z]+").unwrap()).unwrap();
        internal::Mibpaste{inner: inner}
    }
}


mod internal {
    use std::fs;
    use std::io::{self, Write};
    use htmlescape::decode_html;
    use regex::Regex;
    use gist::{self, Gist};
    use hosts::{FetchMode, Host};
    use util::read_lines;

    /// Actual implementation type for Mibpaste,
    /// taking a generic parameter so it can be substituted in tests.
    pub struct Mibpaste<T: Host> {
        pub(super) inner: T,
    }

    impl<T: Host> Host for Mibpaste<T> {
        fn id(&self) -> &'static str { self.inner.id() }
        fn name(&self) -> &str { self.inner.name() }

        /// Fetch the gist from mibpaste.com host.
        ///
        /// The fetched gist will actually be just an HTML page of the gist,
        /// requiring some additional logic to strip the HTML elements
        /// and leave just the raw code.
        fn fetch_gist(&self, gist: &Gist, mode: FetchMode) -> io::Result<()> {
            try!(self.inner.fetch_gist(gist, mode));

            lazy_static! {
                static ref BODY_RE: Regex = Regex::new(r"(?i)<body\s*>").unwrap();
                static ref HR_RE: Regex = Regex::new(r"(?i)<hr\s*[/]?>").unwrap();
                static ref TRAILING_BR_RE: Regex = Regex::new(r"(?i)<br\s*[/]?>\s*$").unwrap();
            }

            // Find where the code of the downloaded HTML gist is.
            // This is most of the <body>, except for the footer separated by a single <hr>.
            let mut gist_lines = try!(read_lines(gist.binary_path()));
            let code_start_idx = gist_lines.iter().position(|l| BODY_RE.is_match(l));
            let code_end_idx = gist_lines.iter().rposition(|l| HR_RE.is_match(l));
            let code_lines = match (code_start_idx, code_end_idx) {
                (Some(start), Some(end)) => {
                    // TODO: test that start < end and signal errors
                    gist_lines.drain(start + 1..end).collect::<Vec<String>>()
                },
                _ => {
                    debug!("{} gist {} is already in raw form, finishing fetch",
                        self.name(), gist.uri);
                    return Ok(());
                }
            };

            // Strip the HTML line breaks and entities,
            // and write the "raw" gist back to the original binary file.
            let raw_lines = code_lines.into_iter()
                .map(|l| TRAILING_BR_RE.replace(&l, "").into_owned())
                .filter(|l| !l.trim().is_empty());
            let mut file = try!(fs::OpenOptions::new()
                .read(false).write(true).truncate(true).create(false)
                .open(gist.binary_path()));
            for line in raw_lines {
                let line = try!(decode_html(&line)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData,
                        format!("{:?}", e))));
                try!(writeln!(file, "{}", line));
            }

            Ok(())
        }

        // Boilerplate pass-through methods.
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
}


#[cfg(test)]
mod tests {
    use super::Mibpaste;

    #[test]
    fn html_url_regex() {
        let host = Mibpaste::new();
        let html_url: String = host.inner.html_url_origin();

        let valid_html_urls: Vec<(/* URL */ String,
                                  /* ID */ &'static str)> = vec![
            (html_url.clone() + "/abc", "abc"),                // short
            (html_url.clone() + "/a1b2c3d4e5", "a1b2c3d4e5"),  // long
            (html_url.clone() + "/43ffg", "43ffg"),            // starts with digit
            (html_url.clone() + "/46417247", "46417247"),      // only digits
            (html_url.clone() + "/MfgT45f", "MfgT45f"),        // mixed case
        ];
        let invalid_html_urls: Vec<String> = vec![
            html_url.clone() + "/a/b/c",            // too many path segments
            html_url.clone() + "/a/",               // trailing slash
            html_url.clone() + "//",                // ID must not be empty
            html_url.clone() + "/",                 // no ID at all
            "http://example.com/fhdFG36ok".into(),  // wrong mibpaste.com domain
            "foobar".into(),                        // not even an URL
        ];

        let html_url_re = host.inner.html_url_regex();
        for (ref valid_url, id) in valid_html_urls {
            let captures = html_url_re.captures(valid_url)
                .expect(&format!("Paste's HTML URL was incorrectly deemed invalid: {}", valid_url));
            assert_eq!(id, &captures["id"]);
        }
        for ref invalid_url in invalid_html_urls {
            assert!(!html_url_re.is_match(invalid_url),
                "URL was incorrectly deemed a valid gist HTML URL: {}", invalid_url);
        }
    }
}
