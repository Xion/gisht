"""
Automation tasks for preparing release bundles with the gisht binary.

This essentially a wrapper script around fpm (https://github.com/jordansissel/fpm).
Requires fpm to be installed first, which may in turn require Ruby with C headers.
Refer to fpm's README for installation instructions.
"""
from itertools import starmap
import logging
from pathlib import Path
import shutil
try:
    from shlex import quote
except ImportError:
    from pipes import quote
import sys

from invoke import task

from tasks.util import cargo


PACKAGE_INFO = dict(
    name="gisht",
    version="1.0",  # TODO: read from Cargo.toml
    description="Gists in the shell",
    url="http://github.com/Xion/gisht",
    license="GPL v3",
    maintainer="Karol Kuczmarski",
)

SOURCE_DIR = Path.cwd() / 'target' / 'release'
BIN = 'gisht'
LICENSE_FILE = Path.cwd() / 'LICENSE'
OUTPUT_DIR = Path.cwd() / 'release'

# Directory where the binary should be installed on Linux systems.
LINUX_INSTALL_DIR = '/usr/bin'


@task(default=True)
def all(ctx):
    """Create all release packages."""
    deb(ctx)
    rpm(ctx)


@task
def deb(ctx):
    """Create the release package for Debian (.deb)."""
    ensure_fpm(ctx)
    ensure_output_dir()
    prepare_release(ctx)

    logging.info("Preparing Debian package...")
    bundle(ctx, 'deb', prefix=LINUX_INSTALL_DIR)
    logging.debug("Debian package created.")


@task
def rpm(ctx):
    """Create the release package for RedHat (.rpm)."""
    ensure_fpm(ctx)
    if not which(ctx, 'rpm'):
        logging.warning("`rpm` not found, cannot create RedHat package.")
        return 1

    ensure_output_dir()
    prepare_release(ctx)

    logging.info("Reparing RedHat package...")
    bundle(ctx, 'rpm', prefix=LINUX_INSTALL_DIR)
    logging.debug("RedHat package created.")


# Shared release stages

def prepare_release(ctx):
    """Prepare release files.

    This includes building the binary in --release mode.
    """
    cargo(ctx, 'build', '--release')

    if which(ctx, 'strip'):
        ctx.run('strip %s' % (SOURCE_DIR / BIN))
    else:
        logging.warning("'strip' not found, binary will retain debug symbols.")

    # Ensure a license file is available in the source directory.
    shutil.copy(str(LICENSE_FILE), str(SOURCE_DIR))


def ensure_output_dir():
    """Ensure that the release directory exists."""
    if OUTPUT_DIR.exists():
        if not OUTPUT_DIR.is_dir():
            logging.error(
                "Output path %s already exists but it's not a directory!",
                OUTPUT_DIR)
            sys.exit(2)
        return

    try:
        logging.info("Creating output directory (%s)...", OUTPUT_DIR)
        OUTPUT_DIR.mkdir(parents=True)
    except IOError as e:
        logging.fatal("Failed to create output directory %s: %s",
                      OUTPUT_DIR, e)
        sys.exit(2)
    else:
        logging.debug("Output directory created.")


def bundle(ctx, target, **flags):
    """Create a release bundle by involing `fpm` with common parameters.

    :param target: Release target, like "deb", "rpm", etc.
    :param flags: Additional flags to be passed to fpm

    :return: Invoke's process object
    """
    # Define the fpm's input and output.
    flags.update(s='dir', C=str(SOURCE_DIR))
    flags.update(
        t=target,
        package=str(OUTPUT_DIR / ('%s.%s' % (PACKAGE_INFO['name'], target))))

    # Provide package information.
    for key, value in PACKAGE_INFO.iteritems():
        flags.setdefault(key, value)
    flags.setdefault('vendor', "<unspecified>")

    # Use Gist SHA of HEAD revision as the --iteration value.
    if 'iteration' not in flags:
        sha = ctx.run('git rev-parse --short HEAD', hide=True).stdout.strip()
        flags['iteration'] = sha

    def format_flag(name, value):
        return '-%s %s' % (name if len(name) == 1 else '-' + name,
                           quote(value))
    fpm_args = ' '.join(starmap(format_flag, flags.iteritems()))
    fpm_cmdline = 'fpm --force --log error %s %s' % (fpm_args, BIN)

    logging.debug("Running %s" % fpm_cmdline)
    return ctx.run(fpm_cmdline)


# Utility functions

def ensure_fpm(ctx):
    """Ensure that fpm is available."""
    if not which(ctx, 'fpm'):
        logging.fatal("fpm not found, aborting.")
        sys.exit(1)


def which(ctx, prog):
    """Runs $ which prog."""
    return ctx.run('which %s' % prog, warn=True, hide=True)
