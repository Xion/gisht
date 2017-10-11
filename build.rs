//! Build script.

extern crate rustc_version;


use std::error::Error;
use std::process::Command;
use std::str;


const MONIKER: &'static str = "gisht";


fn main() {
    match git_head_sha() {
        Ok(rev) => pass_metadata("REVISION", &rev),
        Err(e) => println!("cargo:warning=Failed to obtain current Git SHA: {}", e),
    };
    match compiler_signature() {
        Ok(sig) => pass_metadata("COMPILER", &sig),
        Err(e) => println!("cargo:warning=Failed to obtain compiler information: {}", e),
    };
}

fn pass_metadata(kind: &'static str, data: &str) {
    println!("cargo:rustc-env=X_{}_{}={}",
        MONIKER.to_uppercase(), kind.to_uppercase(), data)
}


fn git_head_sha() -> Result<String, Box<Error>> {
    let mut cmd = Command::new("git");
    cmd.args(&["rev-parse", "--short", "HEAD"]);

    let output = try!(cmd.output());
    let sha = try!(str::from_utf8(&output.stdout[..])).trim().to_owned();
    Ok(sha)
}

fn compiler_signature() -> Result<String, Box<Error>> {
    let rustc = rustc_version::version_meta()?;
    let signature = format!("{channel} {version} on {host}",
        version = rustc.short_version_string,
        channel = format!("{:?}", rustc.channel).to_lowercase(),
        host = rustc.host);
    Ok(signature)
}
