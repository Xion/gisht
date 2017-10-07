//! Module implementing bpaste.net as Basic gist host.

use regex::Regex;

use hosts::common::Basic;


/// bpaste.net host ID.
pub const ID: &'static str = "bp";

/// Create the bpaste.net Host implementation.
pub fn create() -> Basic {
    Basic::new(ID, "bpaste.net",
               "http://bpaste.net/raw/${id}",
               "http://bpaste.net/show/${id}",
               Regex::new("[0-9a-z]+").unwrap()).unwrap()
}


#[cfg(test)]
mod tests {
    use super::create;

    #[test]
    fn html_url_regex() {
        let host = create();
        let html_url: String = host.html_url_origin() + "/show";

        let valid_html_urls: Vec<(/* URL */ String,
                                  /* ID */ &'static str)> = vec![
            (html_url.clone() + "/abc", "abc"),                // short
            (html_url.clone() + "/a1b2c3d4e5", "a1b2c3d4e5"),  // long
            (html_url.clone() + "/43ffg", "43ffg"),            // starts with digit
            (html_url.clone() + "/46417247", "46417247"),      // only digits
        ];
        let invalid_html_urls: Vec<String> = vec![
            html_url.clone() + "/a/b/c",            // too many path segments
            html_url.clone() + "/a/",               // trailing slash
            html_url.clone() + "//",                // ID must not be empty
            html_url.clone() + "/",                 // no ID at all
            html_url.clone() + "/MfgT45f",          // mixed case
            "http://example.com/fhdFG36ok".into(),  // wrong bpaste.net domain
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
