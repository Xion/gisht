//! Module implementing lpaste.net as Basic gist host.

use regex::Regex;

use hosts::common::Basic;


/// lpaste.net host ID.
pub const ID: &'static str = "lp";

/// Create the lpaste.net Host implementation.
pub fn create() -> Basic {
    Basic::new(ID, "lpaste.net",
               "http://lpaste.net/raw/${id}",
               "http://lpaste.net/${id}",
               Regex::new("[0-9]+").unwrap()).unwrap()
}


#[cfg(test)]
mod tests {
    use super::create;

    #[test]
    fn html_url_regex() {
        let host = create();
        let html_url: String = host.html_url_origin();

        let valid_html_urls: Vec<(/* URL */ String,
                                  /* ID */ &'static str)> = vec![
            (html_url.clone() + "/123", "123"),                // short
            (html_url.clone() + "/1234567890", "1234567890"),  // long
            (html_url.clone() + "/09876", "09876"),            // starts with zero
        ];
        let invalid_html_urls: Vec<String> = vec![
            html_url.clone() + "/a/b/c",            // too many path segments
            html_url.clone() + "/a/",               // trailing slash
            html_url.clone() + "//",                // ID must not be empty
            html_url.clone() + "/",                 // no ID at all
            html_url.clone() + "/MfgT45f",          // wrong characters
            "http://example.com/123456789".into(),  // wrong lpaste.net domain
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
