"""
Automation tasks, aided by the Invoke package.
"""
import logging
import os
from pathlib import Path
import sys

from invoke import Collection, task

from tasks.util import ensure_rustc_version, get_cargo_flags, cargo


HELP = {
    'release': "Whether to build the binary in release mode.",
    'verbose': "Whether to show verbose logging output of the build",
}


@task(help=HELP)
def build(ctx, release=False, verbose=False):
    """Build the project."""
    ensure_rustc_version(ctx)
    cargo_build = cargo(ctx, 'build', *get_cargo_flags(release, verbose),
                        pty=True, wait=True)
    if not cargo_build.ok:
        logging.critical("Failed to build the binary")
        return cargo_build.return_code

    # Run the resulting binary to obtain command line help.
    verbose and logging.debug("Running the binary to obtain usage string")
    binary = cargo(ctx, 'run', *get_cargo_flags(release, verbose=False),
                   hide=True, warn=True, wait=True)
    if not (binary.ok or binary.return_code == os.EX_USAGE):
        logging.critical("Compiled binary return error code %s; stderr:\n%s",
                         binary.return_code, binary.stderr)
        return binary.return_code
    help_lines = binary.stderr.strip().splitlines()

    # Beautify it a little before pasting into README.
    while not help_lines[0].startswith("USAGE"):
        del help_lines[0]  # Remove "About" line & other fluff.
    del help_lines[0]  # Remove "USAGE:" header.
    help_lines[0] = help_lines[0].lstrip()  # Unindent the actual usage line.
    help = os.linesep.join('    ' + line for line in help_lines)

    # Paste the modified help into README.
    verbose and logging.info("Updating README to add binary's help string")
    with (Path.cwd() / 'README.md').open('r+t', encoding='utf-8') as f:
        readme_lines = [line.rstrip() for line in f.readlines()]

        # Determine the line indices of the region to replace,
        # which is between the header titled "Usage" and the immediate next one.
        begin_idx, end_idx = None, None
        for i, line in enumerate(readme_lines):
            if not line.startswith('#'):
                continue
            if begin_idx is None:
                if "# Usage" in line:
                    begin_idx = i
            else:
                end_idx = i
                break
        if begin_idx is None or end_idx is None:
            logging.critical("Usage begin or end markers not found in README "
                             "(begin:%s, end:%s)", begin_idx, end_idx)
            return 2

        # Reassemble the modified content of the README, with help inside.
        readme_content = os.linesep.join([
            os.linesep.join(readme_lines[:begin_idx + 1]),
            '', help, '',
            os.linesep.join(readme_lines[end_idx:]),
            '',   # Ensure newline at the end of file.
        ])

        f.seek(0)
        f.truncate()
        f.write(readme_content)


@task(help=HELP)
def clean(ctx, release=False, verbose=False):
    """Clean all build artifacts."""
    cargo(ctx, 'clean', *get_cargo_flags(release, verbose), wait=False)


@task(help=HELP)
def test(ctx, release=False, verbose=False):
    """Run all the tests."""
    ensure_rustc_version(ctx)
    cargo(ctx, 'test', '--no-fail-fast', *get_cargo_flags(release, verbose),
          wait=False)


@task
def run(ctx):
    """Run the binary."""
    # Because we want to accept arbitrary arguments, we have to ferret them out
    # of sys.argv manually.
    cargo(ctx, 'run', *sys.argv[2:], wait=False)


# Task setup

ns = Collection()

ns.add_task(build)
ns.add_task(clean)
ns.add_task(run)
ns.add_task(test, default=True)

from tasks import release
ns.add_collection(release)


# This precondition makes it easier to localize files needed by tasks.
if not os.path.exists(os.path.join(os.getcwd(), '.gitignore')):
    logging.fatal(
        "Automation tasks can only be invoked from project's root directory!")
    sys.exit(1)
