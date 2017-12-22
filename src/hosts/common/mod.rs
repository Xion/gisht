//! Module implementing common gist host patterns, shared by multiple actual hosts.

mod basic;
mod html_only;

mod util;


pub use self::basic::Basic;
pub use self::html_only::HtmlOnly;
