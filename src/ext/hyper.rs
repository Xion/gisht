//! Module extending the capabilities of the Hyper library.

pub mod header {
    //! Additional HTTP headers for use with Hyper.

    use std::collections::HashMap;
    use std::fmt;
    use std::str::from_utf8;

    use hyper;
    use hyper::header::{Header, HeaderFormat};
    use regex::{Regex, RegexBuilder};


    /// Type of a single item in Link: header.
    #[derive(Clone, Debug)]
    pub struct LinkItem {
        /// Value of the rel="" attribute.
        pub rel: String,
        /// URL the Link: item is pointing to.
        pub url: String,
        // TODO: support more/arbitrary attributes
    }

    /// The Link: header.
    /// Wrapped type is a map of rel= attribute values to LinkItems.
    #[derive(Clone, Debug)]
    pub struct Link(pub HashMap<String, LinkItem>);

    impl Header for Link {
        fn header_name() -> &'static str { "Link" }

        fn parse_header(raw: &[Vec<u8>]) -> hyper::Result<Link> {
            lazy_static! {
                static ref RE: Regex = RegexBuilder::new(r#"
                    <(?P<url>[^>]+)>;
                    \s*
                    rel="(?P<rel>\w+)"
                "#)
                .ignore_whitespace(true)
                .compile().unwrap();
            }

            // Note that in case of multiple Link: values with the same rel=,
            // the last one counts, overwriting all the previous ones.
            let mut links = HashMap::new();
            for value in raw {
                let value = try!(from_utf8(value).map_err(|_| hyper::Error::Header));
                if !RE.is_match(value) {
                    return Err(hyper::Error::Header);
                }
                // TODO: split at comma and MATCH against the parts
                for li_cap in RE.captures_iter(value) {
                    let li = LinkItem{rel: li_cap.name("rel").unwrap().to_owned(),
                                      url: li_cap.name("url").unwrap().to_owned()};
                    links.insert(li.rel.clone(), li);
                    // TODO: consider erroring on duplicate rels
                }
            }
            Ok(Link(links))
        }
    }

    impl HeaderFormat for Link {
        fn fmt_header(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
            write!(fmt, "{}", self.0.values()
                .map(|li| format!("<{}>; rel=\"{}\"", li.url, li.rel))
                .collect::<Vec<_>>().join(", "))
        }
    }

    #[cfg(test)]
    mod tests {
        use hyper::header::Header;
        use super::Link;

        #[test]
        fn link_parse_empty() {
            let result = Link::parse_header(&[vec![]]);
            assert!(result.is_err(), "Empty Link: unexpectedly parsed");
        }

        #[test]
        fn link_parse_invalid() {
            let input = "<http://example.com";
            let result = Link::parse_header(&[input.as_bytes().to_vec()]);
            assert!(result.is_err(),
                "Link: header unexpectedly parsed: {}", input);
        }

        #[test]
        fn link_parse_single() {
            let url = "http://example.com";
            let link = Link::parse_header(&[
                format!(r#"<{}>; rel="next""#, url).as_bytes().to_vec()
            ]).unwrap();
            assert_eq!(url, link.0.get("next").unwrap().url);
        }

        #[test]
        fn link_parse_nextprev() {
            let next_url = "http://example.com/next";
            let prev_url= "http://example.com/prev";
            let link = Link::parse_header(&[format!(
                r#"<{}>; rel="next", <{}>; rel="prev""#, next_url, prev_url
            ).as_bytes().to_vec()]).unwrap();
            assert_eq!(next_url, link.0.get("next").unwrap().url);
            assert_eq!(prev_url, link.0.get("prev").unwrap().url);
        }
    }
}
