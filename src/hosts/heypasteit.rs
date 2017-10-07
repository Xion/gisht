//! Module implementing heypasteit.com as gist host.

use regex::Regex;

use hosts::common::Basic;


/// heypasteit.com host ID.
pub const ID: &'static str = "hpi";

/// Create the heypasteit.com Host implementation.
pub fn create() -> Basic {
    Basic::new(ID, "heypasteit.com",
               "http://heypasteit.com/download/${id}",
               "http://heypasteit.com/clip/${id}",
               Regex::new("[0-9A-Z]+").unwrap()).unwrap()
}


#[cfg(test)]
mod tests {
    use super::create;

    #[test]
    fn html_url_regex() {
        let host = create();
        let html_url: String = host.html_url_origin() + "/clip";

        let valid_html_urls: Vec<(/* URL */ String,
                                  /* ID */ &'static str)> = vec![
            (html_url.clone() + "/ABC", "ABC"),                // short
            (html_url.clone() + "/A1B2C3D4E5", "A1B2C3D4E5"),  // long
            (html_url.clone() + "/43FFG", "43FFG"),            // starts with digit
            (html_url.clone() + "/46417247", "46417247"),      // only digits
        ];
        let invalid_html_urls: Vec<String> = vec![
            html_url.clone() + "/a/b/c",            // too many path segments
            html_url.clone() + "/a/",               // trailing slash
            html_url.clone() + "//",                // ID must not be empty
            html_url.clone() + "/",                 // no ID at all
            html_url.clone() + "/MfgT45f",          // mixed case
            "http://example.com/fhdFG36ok".into(),  // wrong heypasteit.com domain
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
