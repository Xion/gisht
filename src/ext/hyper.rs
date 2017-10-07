//! Module extending the capabilities of the Hyper library.

#[allow(dead_code)]
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
        // TODO: support more (arbitrary?) attributes,
        // like those mentioned in the RFC: https://tools.ietf.org/html/rfc5988#page-7
    }

    /// The Link: header.
    /// Wrapped type is a map of rel= attribute values to LinkItems.
    #[derive(Clone, Debug)]
    pub struct Link(HashMap<String, Vec<LinkItem>>);

    impl Link {
        /// Returns all LinkItems associated with given rel=.
        pub fn items<'r>(&self, rel: &'r str) -> &[LinkItem] {
            self.0.get(rel).map(|li| &li[..]).unwrap_or(&[])
        }

        /// Returns a URL corresponding to given rel=, if there is exactly one.
        pub fn url<'r>(&self, rel: &'r str) -> Option<&str> {
            let lis = try_opt!(self.0.get(rel));
            if lis.len() == 1 { Some(&lis[0].url) } else { None }
        }
    }

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
                .build().unwrap();
            }

            let mut links = HashMap::new();
            for value in raw {
                let value = try!(from_utf8(value).map_err(|_| hyper::Error::Header));
                if !RE.is_match(value) {
                    return Err(hyper::Error::Header);
                }
                for li_cap in RE.captures_iter(value) {
                    let li = LinkItem{rel: li_cap["rel"].to_owned(),
                                      url: li_cap["url"].to_owned()};
                    let link_items = links.entry(li.rel.clone()).or_insert_with(|| vec![]);
                    link_items.push(li);
                }
            }
            Ok(Link(links))
        }
    }

    impl HeaderFormat for Link {
        fn fmt_header(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
            write!(fmt, "{}", self.0.values()
                .flat_map(|lis| lis.iter()
                    .map(|li| format!("<{}>; rel=\"{}\"", li.url, li.rel)))
                .collect::<Vec<_>>().join(", "))
        }
    }

    #[cfg(test)]
    mod tests {
        use hyper;
        use hyper::header::Header;
        use super::Link;

        fn parse(s: &str) -> hyper::Result<Link> {
            Link::parse_header(&[s.as_bytes().to_vec()])
        }

        #[test]
        fn link_parse_empty() {
            let result = parse("");
            assert!(result.is_err(), "Empty Link: unexpectedly parsed");
        }

        #[test]
        fn link_parse_invalid() {
            let input = "<http://example.com";
            let result = parse(input);
            assert!(result.is_err(),
                "Link: header unexpectedly parsed: {}", input);
        }

        #[test]
        fn link_parse_single() {
            let url = "http://example.com";
            let link = parse(&format!(r#"<{}>; rel="next""#, url)).unwrap();
            assert_eq!(url, link.url("next").unwrap());
        }

        #[test]
        fn link_parse_nextprev() {
            let next_url = "http://example.com/next";
            let prev_url= "http://example.com/prev";
            let link = parse(&format!(
                r#"<{}>; rel="next", <{}>; rel="prev""#, next_url, prev_url
            )).unwrap();
            assert_eq!(next_url, link.url("next").unwrap());
            assert_eq!(prev_url, link.url("prev").unwrap());
        }

        #[test]
        fn link_parse_duplicate_rel() {
            let stylesheet1 = "/style1.css";
            let stylesheet2 = "/style2.css";
            let link = parse(&format!(
                r#"<{}>; rel="stylesheet", <{}>; rel="stylesheet""#, stylesheet1, stylesheet2
            )).unwrap();

            // Because there is more than one item with this rel=, Link::url returns None.
            assert_eq!(2, link.items("stylesheet").len());
            assert!(link.url("stylesheet").is_none());
        }
    }
}
