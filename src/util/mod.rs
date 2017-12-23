//! Utility module.

use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::str::FromStr;

use hyper::client::{Client, Response};
use hyper::header::ContentLength;
use hyper::net::HttpsConnector;
use hyper_native_tls::NativeTlsClient;
use serde_json::Value as Json;


/// Like try!(), but returns Some(Err(err)) in case of error.
/// Compatible with functions returning Option<Result<T, E>>.
macro_rules! try_some {
    ($ex:expr) => (match $ex {
        Ok(value) => value,
        Err(error) => return Some(Err(error.into())),
    })
}


#[cfg(windows)]
pub const LINESEP: &'static str = "\r\n";
#[cfg(not(windows))]
pub const LINESEP: &'static str = "\n";



/// Create a symlink to a regular file.
#[cfg(unix)]
pub fn symlink_file<S, D>(src: S, dst: D) -> io::Result<()>
    where S: AsRef<Path>, D: AsRef<Path>
{
    if src.as_ref().is_file() {
        ::std::os::unix::fs::symlink(src, dst)
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, format!(
            "cannot create symlink: `{}` is not a regular file", src.as_ref().display()
        )))
    }
}

/// Create a symlink to a regular file.
#[cfg(windows)]
pub fn symlink_file<S, D>(src: S, dst: D) -> io::Result<()>
    where S: AsRef<Path>, D: AsRef<Path>
{
    ::std::os::windows::fs::symlink_file(src, dst)
}


/// Mark a given file path as executable for all users..
pub fn mark_executable<P: AsRef<Path>>(path: P) -> io::Result<()> {
    let metadata = try!(fs::metadata(path.as_ref()));
    if !metadata.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!(
            "cannot mark `{}` as executable as it's not a regular file", path.as_ref().display()
        )));
    }

    // On Unix, the executable bits have to be set on the file.
    if cfg!(unix) {
          use std::os::unix::fs::PermissionsExt;
          let mut perms = metadata.permissions();
          perms.set_mode(0o755);
          return fs::set_permissions(path, perms);
    }

    Ok(())
}


/// Create a TLS-capable HTTP Hyper client.
pub fn http_client() -> Client {
    let ssl = NativeTlsClient::new().unwrap();
    let connector = HttpsConnector::new(ssl);
    Client::with_connector(connector)
}

/// Read HTTP response from hyper and parse it as JSON.
pub fn read_json(response: &mut Response) -> io::Result<Json> {
    let mut body = match response.headers.get::<ContentLength>() {
        Some(&ContentLength(l)) => String::with_capacity(l as usize),
        _ => String::new(),
    };
    response.read_to_string(&mut body).unwrap();
    Json::from_str(&body)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
}
