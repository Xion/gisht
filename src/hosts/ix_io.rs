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
        pub inner: T,
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
                url = format!("{}/{}", url.trim_right_matches("/"), lang);
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
            // * http://ix.io/$ID/$LANG
            // * http://ix.io/$ID+$LANG
            // We need to count the number of path segments to see which case
            // it is (if any).
            let mut url: Cow<str> = url.into();
            let path_segments: Vec<_> = url_obj.path_segments()
                .map(|ps| ps.collect())
                .unwrap_or_else(Vec::new);
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
