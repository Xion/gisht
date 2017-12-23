//! Module implementing common gist host patterns, shared by multiple actual hosts.

mod basic;
mod html_only;

// TODO: this module should probably go out of `common` since it's used
// by standalone gist host implementations too
pub mod util;


pub use self::basic::Basic;
pub use self::html_only::HtmlOnly;
