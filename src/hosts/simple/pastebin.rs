//! Module implementing pastebin.com as gist host.
//!
//! In their parlance, a single gist is a "paste".
//!
//! Although they have an API and there are pastebin user accounts,
//! user names are not a part of pastes' URLs,
//! which is why it can be implemented as a Simple gist host.

use regex::Regex;

use super::Simple;


/// pastebin.com host ID.
pub const ID: &'static str = "pb";

/// Create the Pastebin Host implementation.
pub fn create() -> Simple {
    Simple::new(ID, "Pastebin.com",
                "http://pastebin.com/raw/${id}",
                "http://pastebin.com/${id}",
                Regex::new("[0-9a-zA-Z]+").unwrap())
}


#[cfg(test)]
mod tests {
    use super::create;

    #[test]
    fn html_url_regex() {
        let html_url: String = "http://pastebin.com".into();

        let valid_html_urls: Vec<(/* URL */ String,
                                  /* ID */ &'static str)> = vec![
            (html_url + "/abc", "abc"),                // short
            (html_url + "/a1b2c3d4e5", "a1b2c3d4e5"),  // long
            (html_url + "/43ffg", "43ffg"),            // starts with digit
            (html_url + "/46417247", "46417247"),      // only digits
            (html_url + "/MfgT45f", "MfgT45f"),        // mixed case
        ];
        let invalid_html_urls: Vec<String> = vec![
            html_url + "/a/b/c",                    // too many path segments
            html_url + "/a/",                       // trailing slash
            html_url + "//",                        // ID must not be empty
            html_url + "/",                         // no ID at all
            "http://example.com/fhdFG36ok".into(),  // wrong Pastebin.com domain
            "foobar".into(),                        // not even an URL
        ];

        let host = create();
        let html_url_re = host.html_url_regex();
        for (ref valid_url, id) in valid_html_urls {
            let captures = html_url_regex.captures(valid_url)
                .expect(&format!("Paste's HTML URL was incorrectly deemed invalid: {}", valid_url));
            assert_eq!(id, captures.name("id").unwrap());
        }
        for invalid_url in invalid_html_urls {
            assert!(!html_url_regex.is_match(invalid_url),
                "URL was incorrectly deemed a valid gist HTML URL: {}", invalid_url);
        }
    }
}
