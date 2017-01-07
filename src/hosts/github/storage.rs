//! Module handling the local storage of GitHub gists, including updating them.

use std::fs;
use std::io;
use std::time::{Duration, SystemTime};

use git2;

use gist::{Datum, Gist};
use util::{mark_executable, symlink_file};
use super::{ID, api, git};


lazy_static! {
    /// Minimum interval between updating (git-pulling) of gists.
    static ref UPDATE_INTERVAL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
}

/// Check whether given gist needs to be updated.
///
/// If the time since last update cannot be determined for whatever reason,
/// the function will assume the update is necessary.
pub fn needs_update<G: AsRef<Gist>>(gist: G) -> bool {
    let gist = gist.as_ref();
    trace!("Checking if GitHub gist {} requires an update...", gist.uri);

    let last = match last_update_time(&gist) {
        Ok(time) => time,
        Err(err) => {
            warn!("Couldn't retrieve the last update time of gist {} ({}). \
                   Assuming an update is needed.", gist.uri, err);
            return true;
        },
    };

    let now = SystemTime::now();
    match now.duration_since(last) {
        Ok(duration) => duration > *UPDATE_INTERVAL,
        Err(err) => {
            let millis = err.duration().as_secs() * 1000 + err.duration().subsec_nanos() as u64 / 1000;
            warn!("Last update time of gist {} is in the future ({}ms from now). \
                   Assuming an update is needed.", gist.uri, millis);
            true
        },
    }
}

/// Determine when was the last time a gist has been updated.
fn last_update_time(gist: &Gist) -> io::Result<SystemTime> {
    // Git writes .git/FETCH_HEAD at every pull, so just check its mtime.
    let fetch_head = gist.path().join(".git").join("FETCH_HEAD");
    fs::metadata(&fetch_head).and_then(|m| m.modified())
}


/// Update an already-downloaded gist.
/// Since GitHub gists are Git repositories, this is basically a `git pull`.
pub fn update_gist<G: AsRef<Gist>>(gist: G) -> io::Result<()> {
    let gist = gist.as_ref();
    let path = gist.path();
    assert!(gist.id.is_some(), "Gist {} has unknown GitHub ID!", gist.uri);
    assert!(path.exists(), "Directory for gist {} doesn't exist!", gist.uri);

    trace!("Updating GitHub gist {}...", gist.uri);
    let reflog_msg = Some("gisht-update");
    if let Err(err) = git::pull(&path, "origin", reflog_msg) {
        match err.code() {
            git2::ErrorCode::Conflict => {
                warn!("Conflict occurred when updating gist {}, rolling back...", gist.uri);
                try!(git::reset_merge(&path));
                debug!("Conflicting update of gist {} successfully aborted", gist.uri);
            },
            git2::ErrorCode::Uncommitted => {
                // This happens if the user has themselves modified the gist
                // and their changes would be overwritten by the merge.
                // There isn't much we can do in such a case,
                // as it would lead to loss of user's modifications.
                error!("Uncommitted changes found to local copy of gist {}", gist.uri);
                return Err(git::to_io_error(err));
            },
            git2::ErrorCode::Unmerged => {
                // This may happen if previous versions of the application
                // (which didn't handle merge conflicts) has left a mess.
                warn!("Previous unfinished Git merge prevented update of gist {}", gist.uri);
                debug!("Attempting to rollback old Git merge of gist {}...", gist.uri);
                try!(git::reset_merge(&path));
                info!("Old Git merge of gist {} successfully aborted", gist.uri);
            },
            _ => return Err(git::to_io_error(err)),
        }
    }

    debug!("GitHub gist {} successfully updated", gist.uri);
    Ok(())
}


/// Clone the gist's repo into the proper directory (which must NOT exist).
/// Given Gist object must have the GitHub ID associated with it.
pub fn clone_gist<G: AsRef<Gist>>(gist: G) -> io::Result<()> {
    let gist = gist.as_ref();
    assert!(gist.uri.host_id == ID, "Gist {} is not a GitHub gist!", gist.uri);
    assert!(gist.id.is_some(), "Gist {} has unknown GitHub ID!", gist.uri);
    assert!(!gist.path().exists(), "Directory for gist {} already exists!", gist.uri);

    // Check if the Gist has a clone URL already in its metadata.
    // Otherwise, talk to GitHub to obtain the URL that we can clone the gist from
    // as a Git repository.
    let clone_url = match gist.info(Datum::RawUrl).clone() {
        Some(url) => url,
        None => {
            trace!("Need to get clone URL from GitHub for gist {}", gist.uri);
            let info = try!(api::get_gist_info(&gist.id.as_ref().unwrap()));
            let url = match info.find("git_pull_url").and_then(|u| u.as_string()) {
                Some(url) => url.to_owned(),
                None => {
                    error!("Gist info for {} doesn't contain git_pull_url", gist.uri);
                    return Err(io::Error::new(io::ErrorKind::InvalidData,
                        format!("Couldn't retrieve git_pull_url for gist {}", gist.uri)));
                },
            };
            trace!("GitHub gist #{} has a git_pull_url=\"{}\"",
                gist.id.as_ref().unwrap(), url);
            url
        },
    };

    // Create the gist's directory and clone it as a Git repo there.
    debug!("Cloning GitHub gist from {}", clone_url);
    let path = gist.path();
    try!(fs::create_dir_all(&path));
    try!(git::clone(&clone_url, &path));

    // Make sure the gist's executable is, in fact, executable.
    let executable = gist.path().join(&gist.uri.name);
    try!(mark_executable(&executable));
    trace!("Marked gist file as executable: {}", executable.display());

    // Symlink the main/binary file to the binary directory.
    let binary = gist.binary_path();
    if !binary.exists() {
        try!(fs::create_dir_all(binary.parent().unwrap()));
        try!(symlink_file(&executable, &binary));
        trace!("Created symlink to gist executable: {}", binary.display());
    }

    Ok(())
}
