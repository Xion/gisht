//! Utility module.

use std::io;
use std::path::Path;


/// Create a symlink to a regular file.
#[cfg(unix)]
pub fn symlink_file<S, D>(src: S, dst: D) -> io::Result<()>
    where S: AsRef<Path>, D: AsRef<Path>
{
    if src.as_ref().is_file() {
        ::std::os::unix::fs::symlink(src, dst)
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidData, format!(
            "cannot create symlink: `{}` is not a regular file", src.as_ref().display()
        )))
    }
}

/// Create a symlink to a regular file.
#[cfg(windows)]
pub fn symlink_file<S, D>(src: S, dst: D) -> io::Result<()>
    where S: AsRef<Path>, D: AsRef<Path>
{
    std::os::windows::fs::symlink_file(src, dst)
}
