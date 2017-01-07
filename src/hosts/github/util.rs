//! Utility functions shared by multiple other modules.

use std::io::Read;
use std::str::FromStr;

use hyper::client::Response;
use hyper::header::ContentLength;
use serde_json::Value as Json;


/// Read HTTP response from hyper and parse it as JSON.
pub fn read_json(response: &mut Response) -> Json {
    let mut body = match response.headers.get::<ContentLength>() {
        Some(&ContentLength(l)) => String::with_capacity(l as usize),
        _ => String::new(),
    };
    response.read_to_string(&mut body).unwrap();
    Json::from_str(&body).unwrap()
}
