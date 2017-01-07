//! Module containing Git operations.
//! They are used for cloning & updating GitHub gists.

use std::io;
use std::path::Path;

use git2::{self, Repository, RepositoryState};
use git2::build::CheckoutBuilder;


/// Clone a Git repository from an URL to given path.
pub fn clone<P: AsRef<Path>>(url: &str, path: P) -> io::Result<()> {
    try!(Repository::clone(url, path.as_ref()).map_err(to_io_error));
    Ok(())
}


/// Perform a standard Git "pull" operation.
pub fn pull<P: AsRef<Path>>(repo_path: P,
                            remote: &str,
                            reflog_msg: Option<&str>) -> Result<(), git2::Error> {
    let repo_path = repo_path.as_ref();
    trace!("Doing `git pull` from remote `{}` inside {}", remote, repo_path.display());

    // Since libgit2 is low-level, we have to perform the requisite steps manually,
    // which means:
    // * doing the fetch from origin remote
    // * checking out the (new) HEAD
    let repo = try!(Repository::open(repo_path));
    let mut origin = try!(repo.find_remote(remote));
    try!(origin.fetch(/* refspecs */ &[], /* options */ None, reflog_msg));
    try!(repo.checkout_head(/* options */ None));

    Ok(())
}

/// Reset an ongoing Git merge operation.
///
/// This isn't exactly the same as `git reset --merge`, because local changes to working tree
/// (prior from starting the merge) are not preserved.
/// Since gists are not supposed to be modified locally, this is fine, however.
pub fn reset_merge<P: AsRef<Path>>(repo_path: P) -> io::Result<()> {
    let repo_path = repo_path.as_ref();
    trace!("Resetting the merge inside {}", repo_path.display());

    let repo = try!(Repository::open(repo_path).map_err(to_io_error));
    assert_eq!(RepositoryState::Merge, repo.state(),
        "Tried to reset a merge on a Git repository that isn't in merge state");

    // Reset (--hard) back to HEAD, and then cleanup the repository state
    // so that MERGE_HEAD doesn't exist anymore, effectively aborting the merge.
    || -> Result<(), git2::Error> {
        let head_revspec = try!(repo.revparse("HEAD"));
        let head = head_revspec.to().unwrap();
        let mut checkout = {
            let mut cb = CheckoutBuilder::new();
            cb.force();
            cb
        };
        try!(repo.reset(&head, git2::ResetType::Hard, Some(&mut checkout)));
        repo.cleanup_state()
    }().map_err(to_io_error)
}


// Utility functions

/// Convert a git2 library error to a generic Rust I/P error.
pub fn to_io_error(git_err: git2::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, git_err)
}
