//! Module implementing mibpaste.com as a gist host.

use regex::Regex;
use select::predicate::{Name, Predicate, Text};

use hosts::Host;
use hosts::common::HtmlOnly;


/// mibpaste.com host ID.
pub const ID: &'static str = "mib";


/// Create the Mibbit host implementation.
///
/// This host is somewhat special insofar that it doesn't seem to allow
/// creating pastes independently. The pastes come from the Mibbit
/// web IRC client.
#[inline]
pub fn create() -> Box<Host> {
    Box::new(
        HtmlOnly::new(ID, "Mibbit",
                      "http://mibpaste.com/${id}",
                      Regex::new("[0-9a-zA-Z]+").unwrap(),
                      Name("body").descendant(Text)).unwrap()
    )
}


#[cfg(test)]
mod tests {
    use select::predicate::Any;
    use traitobject;
    use hosts::common::HtmlOnly;
    use super::create;

    #[test]
    fn html_url_regex() {
        // Create the host and extract a usable pointer to the underlying implementation
        // by unpacking the trait object.
        // (Note that this is doubly unsafe since we assume HtmlOnly is parameterized
        //  with `Any` predicate while in theory it could be anything.
        //  This works because HtmlOnly is repr(C) while testing
        //  and the predicate is last in the struct so we don't care about its size).
        //
        // TODO: eliminate all this unsafe nonsense when `impl Trait` is stable
        let host_box = create();
        let host: &HtmlOnly<Any> = unsafe {
            &*(traitobject::data(&*host_box as *const _) as *const _)
        };

        let html_url: String = host.html_url_origin();

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

        let html_url_re = host.html_url_regex();
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
