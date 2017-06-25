"""
Utility functions used by automation tasks.
"""
from itertools import chain
import logging
import os
from pathlib import Path
try:
    from shlex import quote
except ImportError:
    from pipes import quote

from invoke.exceptions import Exit
import semver
import toml


__all__ = ['ensure_rustc_version',
           'get_cargo_flags', 'cargo',
           'read_cargo_toml']


MIN_RUSTC_VERSION = '1.18.0'


def ensure_rustc_version(ctx):
    """Terminates the build unless the Rust compiler is recent enough."""
    cmd = 'rustc --version'
    rustc_v = ctx.run(cmd, hide=True, warn=True)
    if not rustc_v.ok:
        logging.critical("Rust compiler not found, aborting build.")
        raise Exit(rustc_v.return_code)

    try:
        _, version = rustc_v.stdout.split(None)[:2]
    except ValueError:
        logging.error("Unexpected output from `%s`: %s", cmd, rustc_v.stdout)
        raise Exit(2)

    if not semver.match(version, '>=' + MIN_RUSTC_VERSION):
        logging.error("Build requires at least Rust %s, found %s",
                      MIN_RUSTC_VERSION, version)
        raise Exit(1)

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

    # Obtain Git SHA to pass it as environment variable to Cargo,
    # so that it can be read in the binary code via env!() macro.
    # TODO: consider making this part into a build script so it works with
    # bare Cargo invocations, too
    env = {}
    git_sha = ctx.run('git rev-parse HEAD', warn=True, hide=True)
    if git_sha.ok:
        env['X_CARGO_REVISION'] = git_sha.stdout.strip()
    else:
        logging.warning(
            "Cannot obtain Git SHA to save as revision being built: %s",
            git_sha.stderr or git_sha.stdout)

    wait = kwargs.pop('wait', True)
    if wait:
        kwargs.setdefault('env', {}).update(env)
        return ctx.run('cargo ' + ' '.join(map(quote, cargo_args)), **kwargs)
    else:
        argv = ['cargo'] + cargo_args  # execvpe() needs explicit argv[0]
        env.update(os.environ)
        os.execvpe(argv[0], argv, env)


def read_cargo_toml(key, manifest=None):
    """Read a value from Cargo.toml manifest.

    :param key: Key to read from [package], or entire path to a key.
                It may contain dots.
    :param manifest: Optional path to the manifest,
                     or a file-like object with it opened

    :return: The value
    """
    if not isinstance(key, (list, tuple)):
        key = (key,) if key.startswith('package.') else ('package', key)
    key = list(chain.from_iterable(k.split('.') for k in key))
    if not key:
        raise ValueError("key must not be empty")

    # Read the content of Cargo.toml.
    manifest = manifest or Path.cwd() / 'Cargo.toml'
    if hasattr(manifest, 'read'):
        content = toml.load(manifest)
    else:
        manifest = Path(manifest)
        with manifest.open() as f:
            content = toml.load(f)

    # Get the value.
    value = content
    for k in key:
        value = value[k]
    return value
