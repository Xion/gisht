//! Module implementing ix.io as a gist host.

use regex::Regex;

use hosts::common::Basic;


/// ix.io gist host.
pub const ID: &'static str = "ix";


pub type Ix = internal::Ix<Basic>;

impl Ix {
    #[inline]
    pub fn new() -> Self {
        // Similarly to Hastebin, ix.io URLs may have the language added
        // to the URL. We will strip it before handing over to the Basic host,
        // and then re-add it when generating HTML URL.
        let inner = Basic::new(ID, "ix.io",
                               "http://ix.io/${id}",
                               "http://ix.io/${id}/",  // Yes, just a slash.
                               Regex::new("[0-9a-z]+").unwrap()).unwrap();
        internal::Ix{inner: inner}
    }
}


mod internal {
    use std::borrow::Cow;
    use std::io;

    use url::{self, Url};

    use gist::{self, Datum, Gist};
    use hosts::{FetchMode, Host};

    /// Actual implementation type for ix.io,
    /// taking a generic parameter so it can be substituted in tests.
    pub struct Ix<T: Host> {
        pub(super) inner: T,
    }

    impl<T: Host> Host for Ix<T> {
        fn id(&self) -> &'static str { self.inner.id() }
        fn name(&self) -> &str { self.inner.name() }

        fn fetch_gist(&self, gist: &Gist, mode: FetchMode) -> io::Result<()> {
            self.inner.fetch_gist(gist, mode)
        }

        /// Return the URL to given ix.io gist.
        fn gist_url(&self, gist: &Gist) -> io::Result<String> {
            let mut url = try!(self.inner.gist_url(gist));

            // Add language to the URL.
            if let Some(ref lang) = gist.info(gist::Datum::Language) {
                url = format!("{}/{}/", url.trim_right_matches("/"), lang);
            }
            Ok(url)
        }

        fn gist_info(&self, gist: &Gist) -> io::Result<Option<gist::Info>> {
            self.inner.gist_info(gist)
        }

        /// Resolve given URL as potentially pointing to a sprunge.us gist.
        fn resolve_url(&self, url: &str) -> Option<io::Result<Gist>> {
            let url_obj = try_opt!(Url::parse(url).ok());

            // Check the correct domain manually first,
            // as we'll be doing some additional processing.
            if url_obj.host() != Some(url::Host::Domain("ix.io")) {
                debug!("URL {} doesn't point to an ix.io gist", url);
                return None;
            }

            // According to ix.io homepage, there are two ways the URL can
            // contain language information:
            // * http://ix.io/$ID/$LANG/
            // * http://ix.io/$ID+$LANG/
            // We need to count the number of path segments to see which case
            // it is (if any).
            let mut url: Cow<str> = url.into();
            let mut path_segments: Vec<_> = url_obj.path_segments()
                .map(|ps| ps.collect())
                .unwrap_or_else(Vec::new);
            if path_segments.last() == Some(&"") {
                // URL had a trailing slash. That's expected, actually,
                // but we don't want the empty path segment that follows it.
                path_segments.pop();
            }
            let ps_count = path_segments.len();

            // Determine the language from the path segment pattern,
            // and remove it from the URL for resolving.
            let lang: Option<&str> = match ps_count {
                1 => {
                    let id_parts: Vec<_> = path_segments[0].splitn(2, "-").collect();
                    if id_parts.len() == 2 {
                        trace!("Treating the URL as http://ix.io/$ID-$LANG");
                        let lang = id_parts[1];
                        // Trim the language part but ensure trailing slash.
                        url = format!("{}/",
                            url.trim_right_matches("/")
                                .trim_right_matches(lang).trim_right_matches("-"))
                            .into();
                        Some(lang)
                    }
                    else {
                        trace!("Treating the URL as http://ix.io/$ID");
                        None
                    }
                }
                2 => {
                    trace!("Treating the URL as http://ix.io/$ID/$LANG");
                    let lang = path_segments[1];
                    // Trim the language path segment, but ensure trailing slash.
                    url = format!("{}/",
                        url.trim_right_matches("/").trim_right_matches(lang)
                            .trim_right_matches("/"))
                        .into();
                    Some(lang)
                }
                _ => {
                    warn!("Spurious format of ix.io URL: {}", url);
                    None
                }
            };

            // Resolve the URL using the wrapped method
            // and include the language in gist info.
            trace!("Resolving ix.io URL: {}", url);
            let mut gist = match self.inner.resolve_url(&*url) {
                Some(Ok(gist)) => gist,
                other => return other,
            };
            if let Some(lang) = lang {
                trace!("Adding language to ix.io gist: {}", lang);
                let info_builder = gist.info.clone()
                    .map(|i| i.to_builder()).unwrap_or_else(gist::InfoBuilder::new);
                gist.info = Some(info_builder.with(Datum::Language, lang).build());
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
    use super::{ID, internal, Ix};

    #[test]
    fn html_url_regex() {
        let host = Ix::new();
        let html_url: String = host.inner.html_url_origin();

        let valid_html_urls: Vec<(/* URL */ String,
                                  /* ID */ &'static str)> = vec![
            (html_url.clone() + "/abc/", "abc"),                // short
            (html_url.clone() + "/a1b2c3d4e5/", "a1b2c3d4e5"),  // long
            (html_url.clone() + "/43ffg/", "43ffg"),            // starts with digit
            (html_url.clone() + "/46417247/", "46417247"),      // only digits
        ];
        let invalid_html_urls: Vec<String> = vec![
            html_url.clone() + "/a/b/c",            // too many path segments
            html_url.clone() + "/a",                // no trailing slash
            html_url.clone() + "//",                // ID must not be empty
            html_url.clone() + "/",                 // no ID at all
            html_url.clone() + "/MfgT45f/",         // mixed case
            "http://example.com/fhdFG36ok/".into(), // wrong ix.io domain
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
        let host = internal::Ix{inner: InMemoryHost::with_id(ID)};

        let gist_id = "tea";
        let lang = "bash";
        host.inner.put_gist_with_url(
            Gist::new(gist::Uri::from_name(ID, gist_id).unwrap(), gist_id),
            format!("http://ix.io/{}/", gist_id));

        // Gist resolved against a URL with language should have the language
        // in its info (but of course not in its ID).
        let gist = host.resolve_url(
            &format!("http://ix.io/{}/{}/", gist_id, lang)).unwrap().unwrap();
        assert_eq!(Some(gist_id), gist.id.as_ref().map(String::as_str));
        assert_eq!(Some(lang),
            gist.info(gist::Datum::Language).as_ref().map(String::as_str));
    }

    #[test]
    fn resolve_url_errors_on_broken_url() {
        let host = internal::Ix{inner: InMemoryHost::with_id(ID)};

        let url = "http://ix.io/borked/";
        host.inner.put_broken_url(url);

        let result = host.resolve_url(url).unwrap();
        assert!(result.is_err(), "Resolving a broken URL unexpectedly succeeded");
        let error_msg = format!("{}", result.unwrap_err());
        assert!(error_msg.contains(url),
            "Error message didn't contain the URL `{}`", url);
    }

    #[test]
    fn gist_url_includes_language() {
        let host = internal::Ix{inner: InMemoryHost::with_id(ID)};

        // Add a gist with language.
        let gist_id = "tea";
        let lang = "bash";
        let gist = Gist::new(gist::Uri::from_name(ID, gist_id).unwrap(), gist_id)
            .with_info(gist::InfoBuilder::new()
                .with(gist::Datum::Language, lang)
                .build());
        host.inner.put_gist_with_url(gist.clone(), format!("http://ix.io/{}/", gist_id));

        // Gist URL should include the language in the path.
        let url = host.gist_url(&gist).unwrap();
        assert_eq!(format!("http://ix.io/{}/{}/", gist_id, lang), url);
    }
}
