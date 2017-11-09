//! Module implementing commands that do not operate on gists.

use exitcode::{self, ExitCode};

use hosts::HOSTS;


pub fn list_hosts() -> ExitCode {
    if !HOSTS.is_empty() {
        let longest_id_len = HOSTS.keys().map(|k| k.len()).max().unwrap();
        for host in HOSTS.values() {
            // TODO: display the URL format of the gist host
            println!("{:id_width$} :: {}",
                host.id(), host.name(), id_width=longest_id_len);
        }
    }
    exitcode::OK
}
