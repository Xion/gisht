//! Module implementing thepasteb.in as gist host.

use regex::Regex;

use hosts::common::Basic;


/// thepasteb.in host iD.
pub const ID: &'static str = "tpb";

/// Craate thepasteb.in host implementation.
pub fn create() -> Basic {
    Basic::new(ID, "thepasteb.in",
               "https://thepasteb.in/raw/${id}",
               "https://thepasteb.in/p/${id}",
               Regex::new("[0-9a-zA-Z]+").unwrap()).unwrap()
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
            (html_url.clone() + "/p/abc", "abc"),                // short
            (html_url.clone() + "/p/a1b2c3d4e5", "a1b2c3d4e5"),  // long
            (html_url.clone() + "/p/43ffg", "43ffg"),            // starts with digit
            (html_url.clone() + "/p/46417247", "46417247"),      // only digits
            (html_url.clone() + "/p/MfgT45f", "MfgT45f"),        // mixed case
        ];
        let invalid_html_urls: Vec<String> = vec![
            html_url.clone() + "/a/b/c",            // too many path segments
            html_url.clone() + "/p/abc/",           // trailing slash
            html_url.clone() + "/p//",              // ID must not be empty
            html_url.clone() + "/p",                // no ID at all
            html_url.clone() + "/",                 // no path at all
            "http://example.com/fhdFG36ok".into(),  // wrong thepasteb.in domain
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
