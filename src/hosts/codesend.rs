//! Module implementing the codesend.com gist host.

use regex::Regex;
use select::predicate::{Attr, Name, Predicate};

use hosts::Host;
use hosts::common::HtmlOnly;


/// codesend.com host ID.
pub const ID: &'static str = "cs";


/// Create the CodeSend host implementation.
pub fn create() -> Box<Host> {
    Box::new(
        HtmlOnly::new(ID, "CodeSend",
                      "http://www.codesend.com/view/${id}/",
                      Regex::new("[0-9a-z]+").unwrap(),
                      Name("pre").and(Attr("id", "viewer"))).unwrap()
    )
}
