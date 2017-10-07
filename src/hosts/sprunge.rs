//! Module implementing sprunge.us as gist host.

use regex::Regex;

use hosts::common::Basic;


/// sprunge.us host ID.
pub const ID: &'static str = "spr";


/// sprunge.us as a gist host.
pub type Sprunge = internal::Sprunge<Basic>;

impl Sprunge {
    pub fn new() -> Self {
        // We use the same URL pattern for both raw & HTML URLs,
        // but for the HTML one, we will also try to include a ?<lang> query string,
        // e.g. http://sprunge.us/ABcdEF?py
        let url_pattern = "http://sprunge.us/${id}";

        let inner = Basic::new(ID, "sprunge.us",
                               url_pattern, url_pattern,
                               Regex::new("[0-9a-zA-Z]+").unwrap()).unwrap();
        internal::Sprunge{inner: inner}
    }
}


mod internal {
    use std::io;

    use url::Url;

    use gist::{self, Datum, Gist};
    use hosts::{FetchMode, Host};


    /// Actual implementation type for sprunge.us,
    /// taking a generic parameter so it can be substituted in tests.
    pub struct Sprunge<T: Host> {
        pub(super) inner: T,
    }

    impl<T: Host> Host for Sprunge<T> {
        fn id(&self) -> &'static str { self.inner.id() }
        fn name(&self) -> &str { self.inner.name() }

        fn fetch_gist(&self, gist: &Gist, mode: FetchMode) -> io::Result<()> {
            self.inner.fetch_gist(gist, mode)
        }

        /// Return the URL to given sprunge.us gist.
        fn gist_url(&self, gist: &Gist) -> io::Result<String> {
            let url = try!(self.inner.gist_url(gist));
            let mut url_obj = try!(Url::parse(&url)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

            // Add the language identifier to the URL as query string.
            if url_obj.query().unwrap_or("").is_empty() {
                if let Some(ref lang) = gist.info(Datum::Language) {
                    url_obj.set_query(Some(lang));
                }
            }
            Ok(url_obj.to_string())
        }

        fn gist_info(&self, gist: &Gist) -> io::Result<Option<gist::Info>> {
            self.inner.gist_info(gist)
        }

        /// Resolve given URL as potentially pointing to a sprunge.us gist.
        fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
            let mut url_obj = try_opt!(Url::parse(url).ok());

            // The URL may have a query string part which indicates the language.
            // Strip it for resolving, but preserve for adding it back as Datum::Language
            // to gist::Info.
            let mut lang: Option<String> = None;
            let query = url_obj.query().unwrap_or("")
                .to_owned();  // WTB non-lexical borrows -_-
                              // (w/o this, `query` ref. would "live" until `set_query` below)
            if query.len() > 0 {
                lang = Some(query);
                url_obj.set_query(None);
            }

            // Resolve the URL using the wrapped method and include the language in gist info.
            let mut gist = match self.inner.resolve_url(url_obj.as_str()) {
                Some(Ok(gist)) => gist,
                other => return other,
            };
            if let Some(lang) = lang {
                let info_builder = gist.info.clone()
                    .map(|i| i.to_builder()).unwrap_or_else(gist::InfoBuilder::new);
                gist.info = Some(info_builder.with(Datum::Language, &lang).build());
            }

            Some(Ok(gist))
        }
    }
}


#[cfg(test)]
mod tests {
    use gist::{self, Gist};
    use hosts::Host;
    use testing::InMemoryHost;
    use super::{ID, internal, Sprunge};

    #[test]
    fn html_url_regex() {
        let host = Sprunge::new();
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
            "http://example.com/fhdFG36ok".into(),  // wrong sprunge.us domain
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

    #[test]
    fn resolve_url_recognizes_language() {
        let host = internal::Sprunge{inner: InMemoryHost::with_id(ID)};

        let gist_id = "A46gBeV";
        let lang = "py";
        host.inner.put_gist_with_url(
            Gist::new(gist::Uri::from_name(ID, gist_id).unwrap(), gist_id),
            format!("http://sprunge.us/{}", gist_id));  // no language here

        // Gist resolved against the URL with language should have the language
        // in its info (but of course not in its ID).
        let gist = host.resolve_url(
            &format!("http://sprunge.us/{}?{}", gist_id, lang)).unwrap().unwrap();
        assert_eq!(gist_id, gist.id.as_ref().unwrap());
        assert_eq!(lang, gist.info(gist::Datum::Language).unwrap());
    }

    #[test]
    fn resolve_url_errors_on_broken_url() {
        let host = internal::Sprunge{inner: InMemoryHost::with_id(ID)};

        let url = "http://sprunge.us/borked";
        host.inner.put_broken_url(url);

        let result = host.resolve_url(url).unwrap();
        assert!(result.is_err(), "Resolving a broken URL unexpectedly succeeded");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains(url),
            "Error message didn't contain the URL `{}`", url);
    }

    #[test]
    fn gist_url_includes_language() {
        let host = internal::Sprunge{inner: InMemoryHost::with_id(ID)};

        // Add a gist with language.
        let gist_id = "A46gBeV";
        let lang = "py";
        let gist = Gist::new(gist::Uri::from_name(ID, gist_id).unwrap(), gist_id)
            .with_info(gist::InfoBuilder::new()
                .with(gist::Datum::Language, lang)
                .build());
        host.inner.put_gist_with_url(gist.clone(), format!("http://sprunge.us/{}", gist_id));

        // Gist URL should include the language in its query string.
        let url = host.gist_url(&gist).unwrap();
        assert_eq!(format!("http://sprunge.us/{}?{}", gist_id, lang), url);
    }
}
