//! Utility module.

use std::fs;
use std::io;
use std::path::Path;


/// Like try!(), but the return Some(Err(err)) in case of error.
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


// Module defining standard exit codes that are normally found in POSIX header files.
pub mod exitcode {
    /// Type of the exit codes.
    /// This should be the same as the argument type of std::process::exit.
    pub type ExitCode = i32;

    pub const EX_OK: ExitCode = 0;
    pub const EX_USAGE: ExitCode = 64;
    pub const EX_NOINPUT: ExitCode = 66;
    pub const EX_UNAVAILABLE: ExitCode = 69;
    pub const EX_OSFILE: ExitCode = 72;
    pub const EX_IOERR: ExitCode = 74;
    pub const EX_TEMPFAIL: ExitCode = 75;
}
