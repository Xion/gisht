"""
Automation tasks, aided by the Invoke package.
"""
import logging
import os
try:
    from shlex import quote
except ImportError:
    from pipes import quote
import sys

from invoke import Collection, task
import semver


MIN_RUSTC_VERSION = '1.12.0'

HELP = {
    'release': "Whether to build the binary in release mode.",
    'verbose': "Whether to show verbose logging output of the build",
}


@task(help=HELP)
def build(ctx, release=False, verbose=False):
    """Build the project."""
    ensure_rustc_version(ctx)
    cargo(ctx, 'build', *get_cargo_flags(release, verbose), wait=False)


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


# Utility functions

def ensure_rustc_version(ctx):
    """Terminates the build unless the Rust compiler is recent enough."""
    rustc_v = ctx.run('rustc --version', hide=True)
    if not rustc_v.ok:
        logging.critical("Rust compiler not found, aborting build.")
        sys.exit(rustc_v.return_code)

    _, version, _ = rustc_v.stdout.split(None, 2)
    if not semver.match(version, '>=' + MIN_RUSTC_VERSION):
        logging.error("Build requires at least Rust %s, found %s",
                      MIN_RUSTC_VERSION, version)
        sys.exit(1)

    return True


def get_cargo_flags(release, verbose):
    """Return a list of Cargo flags corresponding to given params."""
    flags = []
    if release:
        flags.append('--release')
    if verbose:
        flags.append('--verbose')
    return flags


def cargo(ctx, cmd, *args, **kwargs):
    """Run Cargo as if inside the specified crate directory.

    :param ctx: Invoke's Context object
    :param cmd: Cargo command to run

    The following are optional keyword arguments:

    :param wait: Whether to wait for the Cargo process to finish (True),
                 or to replace the whole Invoke process with it (False)

    :return: If wait=True, the Invoke's Result object of the run.
    """
    cargo_args = [cmd]
    cargo_args.extend(args)

    wait = kwargs.pop('wait', True)
    if wait:
        return ctx.run('cargo ' + ' '.join(map(quote, cargo_args)), **kwargs)
    else:
        argv = ['cargo'] + cargo_args  # execvp() needs explicit argv[0]
        os.execvp('cargo', argv)


# Task setup

ns = Collection()
ns.add_task(build)
ns.add_task(clean)
ns.add_task(run)
ns.add_task(test, default=True)


# This precondition makes it easier to localize files needed by tasks.
if not os.path.exists(os.path.join(os.getcwd(), '.gitignore')):
    logging.fatal(
        "Automation tasks can only be invoked from project's root directory!")
    sys.exit(1)
