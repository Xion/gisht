//! Utility module.

use std::fs;
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
