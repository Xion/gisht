"""
Build tasks.
"""
import logging
import os
from pathlib import Path

from invoke import task
from invoke.exceptions import Exit

from tasks.util import ensure_rustc_version, cargo, get_cargo_flags


HELP = {
    'release': "Whether to build the binary in release mode.",
    'verbose': "Whether to show verbose logging output of the build",
}


@task(default=True, help=HELP)
def all(ctx, release=False, verbose=False):
    """Build all parts of the project."""
    bin(ctx, release=release, verbose=verbose)
    readme(ctx, release=release, verbose=verbose)


@task(help=HELP)
def bin(ctx, release=False, verbose=False):
    """Build the project's binary."""
    ensure_rustc_version(ctx)
    cargo(ctx, 'build', *get_cargo_flags(release, verbose), pty=True)


@task(pre=[bin], help=HELP)
def readme(ctx, release=False, verbose=False):
    """"Build" the project's README.

    What it means is making sure the usage string in the # Usage section of it
    is up-to-date with respect to the actual output produced by the binary.
    """
    # Run the resulting binary to obtain command line help.
    verbose and logging.debug("Running the binary to obtain usage string")
    binary = cargo(ctx, 'run', *get_cargo_flags(release, verbose=False),
                   hide=True, warn=True, wait=True)
    if not (binary.ok or binary.return_code == os.EX_USAGE):
        logging.critical("Compiled binary return error code %s; stderr:\n%s",
                         binary.return_code, binary.stderr)
        raise Exit(binary.return_code)
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
            raise Exit(2)

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
