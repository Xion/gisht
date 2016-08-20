extern crate hyper;


use std::io::Read;

use hyper::header::ContentLength;


fn main() {
    let client = hyper::Client::new();
    let mut resp = client.get("http://www.mocky.io/v2/57b7d0e1110000d3018dedc4").send().unwrap();
    assert_eq!(resp.status, hyper::Ok);

    let mut body = match resp.headers.get::<ContentLength>() {
        Some(&ContentLength(l)) => String::with_capacity(l as usize),
        _ => String::new(),
    };
    resp.read_to_string(&mut body).unwrap();
    println!("{}", body);
}
