//! Module implementing dpaste.de as gist host.

use regex::Regex;

use hosts::common::Basic;


/// dpaste.de host iD.
pub const ID: &'static str = "dp";

/// Create the dpaste.de host implementation.
pub fn create() -> Basic {
    Basic::new(ID, "dpaste.de",
               "https://dpaste.de/${id}/raw",
               "https://dpaste.de/${id}",
               Regex::new("[A-Za-z]+").unwrap()).unwrap()
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
            (html_url.clone() + "/abc", "abc"),                // short
            (html_url.clone() + "/abcdef", "abcdef"),          // long
            (html_url.clone() + "/aAbBcCdDeE", "aAbBcCdDeE"),  // mixed case
        ];
        let invalid_html_urls: Vec<String> = vec![
            html_url.clone() + "/43ffg",            // has digits
            html_url.clone() + "/a/b/c",            // too many path segments
            html_url.clone() + "/a/",               // trailing slash
            html_url.clone() + "//",                // ID must not be empty
            html_url.clone() + "/",                 // no ID at all
            "http://example.com/fhdFG36ok".into(),  // wrong dpaste.de domain
            "foobar".into(),                        // not even an URL
        ];

        let html_url_re = host.html_url_regex();
        for (ref valid_url, id) in valid_html_urls {
            let captures = html_url_re.captures(valid_url)
                .expect(&format!("Paste's HTML URL was incorrectly deemed invalid: {}", valid_url));
            assert_eq!(id, captures.name("id").unwrap());
        }
        for ref invalid_url in invalid_html_urls {
            assert!(!html_url_re.is_match(invalid_url),
                "URL was incorrectly deemed a valid gist HTML URL: {}", invalid_url);
        }
    }
}
