"""
Automation tasks, aided by the Invoke package.
"""
import logging
from pathlib import Path
import sys

from invoke import Collection, task

from tasks.util import ensure_rustc_version, get_cargo_flags, cargo


HELP = {
    'release': "Whether to run Cargo in release mode.",
    'verbose': "Whether to show verbose logging output of the build",
}


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

ns.add_task(clean)
ns.add_task(run)
ns.add_task(test, default=True)

from tasks import build, release
ns.add_collection(build)
ns.add_collection(release)


# This precondition makes it easier to localize files needed by tasks.
if not Path.cwd().joinpath('.gitignore').exists():
    logging.fatal(
        "Automation tasks can only be invoked from project's root directory!")
    sys.exit(1)
