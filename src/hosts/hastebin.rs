//! Module implementing hastebin.com as gist host.

use regex::Regex;

use hosts::common::Basic;


/// hastebin.com host ID.
pub const ID: &'static str = "hb";

/// Create the hastebin.com Host implementation.
pub fn create() -> Basic {
    // TODO: In reality, the URLs seem to include a completely optional "extension",
    // so the actual URLs can be something like http://hastebin.com/geuyfgdf.foo,
    // where ".foo" is optional indicator of the syntax highlighting
    // to use when displaying the gist in the browser.
    //
    // To support that, we may need to wrap Basic in a new type.
    // For maximum functionality, we'd also have to recreate the original "extension",
    // so that the syntax highlighting can be applied to a website opened via
    // `gisht show hb:ahgfuehg.foo`.
    //
    // Alternatively, just store the extension as part of the gist ID.
    // The downside is potentially having multiple copies of the same gist,
    // under abcdef.foo and abcdef.bar.

    Basic::new(ID, "hastebin.com",
               "https://hastebin.com/raw/${id}",
               "https://hastebin.com/${id}",
               Regex::new("[a-z]+").unwrap()).unwrap()
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
            (html_url.clone() + "/abcdefghij", "abcdefghij"),  // long
        ];
        let invalid_html_urls: Vec<String> = vec![
            html_url.clone() + "/a/b/c",            // too many path segments
            html_url.clone() + "/a/",               // trailing slash
            html_url.clone() + "//",                // ID must not be empty
            html_url.clone() + "/",                 // no ID at all
            html_url.clone() + "/43ffg",            // contains digits
            html_url.clone() + "/MfgTf",            // mixed case
            "http://example.com/fhdgfsgok".into(),  // wrong hastebin.com domain
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
