"""
Utility functions used by automation tasks.
"""
import logging
import os
try:
    from shlex import quote
except ImportError:
    from pipes import quote
import sys

import semver


__all__ = ['ensure_rustc_version', 'get_cargo_flags', 'cargo']


MIN_RUSTC_VERSION = '1.12.0'


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
