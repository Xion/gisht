extern crate clap;
extern crate conv;
extern crate fern;
extern crate hyper;
#[macro_use]
extern crate log;
extern crate rustc_serialize;


mod args;
mod logging;


use std::io::Read;

use hyper::header::ContentLength;
use rustc_serialize::json::Json;


fn main() {
    let opts = args::parse();
    logging::init(opts.verbose()).unwrap();

    // TODO: replace with actual functionality
    let resp = get_http_json("http://www.mocky.io/v2/57b7d0e1110000d3018dedc4");
    let (key, value) = resp.as_object().unwrap().into_iter().next().unwrap();
    info!("{} {}", key, value.as_string().unwrap());
}


// TODO: error handling
fn get_http_json(url: &str) -> Json {
    let resp = get_http_string(url);
    Json::from_str(&resp).unwrap()
}

fn get_http_string(url: &str) -> String {
    let client = hyper::Client::new();
    let mut resp = client.get(url).send().unwrap();

    let mut body = match resp.headers.get::<ContentLength>() {
        Some(&ContentLength(l)) => String::with_capacity(l as usize),
        _ => String::new(),
    };
    resp.read_to_string(&mut body).unwrap();

    body
}
