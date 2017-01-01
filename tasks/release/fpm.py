"""
Automation tasks for preparing release bundles with the gisht binary.

This essentially a wrapper script around fpm (https://github.com/jordansissel/fpm).
Requires fpm to be installed first, which may in turn require Ruby with C headers.
Refer to fpm's README for installation instructions.
"""
import gzip
from itertools import starmap
import logging
import os
from pathlib import Path
import shutil
try:
    from shlex import quote
except ImportError:
    from pipes import quote
from tempfile import mkstemp

from invoke import task
from invoke.exceptions import Exit
import toml

from tasks import BIN
from tasks.release import RELEASE_DIR


# Package information.
#
# Fields that map to tuples are retrieved from the corresponding field path
# in Cargo.toml (and optionally undergo transformations through functions).
PACKAGE_INFO = dict(
    name=('package', 'name'),
    version=('package', 'version'),
    description=('package', 'description'),
    url=('package', 'homepage'),
    license=('package', 'license'),
    maintainer=('package', 'authors', 0,
                lambda v: v[:v.find('<') - 1] if '<' in v else v),
)

SOURCE_DIR = Path.cwd() / 'target' / 'release'
OUTPUT_DIR = RELEASE_DIR

# Extra files to include in the release bundle.
# (Paths are relative to project root).
EXTRA_FILES = ['LICENSE', 'README.md', 'target/release/complete/*']

# Directory where the binary should be installed on Linux systems.
LINUX_INSTALL_DIR = '/usr/bin'


@task
def tar(ctx):
    """Create a release tarball."""
    ensure_fpm(ctx)
    ensure_output_dir()
    prepare_release(ctx)

    logging.info("Preparing release tarball...")
    bundle(ctx, 'tar')
    logging.debug("Release tarball created.")


@task
def tar_gz(ctx):
    """Create a gzip-compressed release tarball."""
    ensure_fpm(ctx)
    ensure_output_dir()
    prepare_release(ctx)

    logging.info("Preparing compressed release tarball...")

    # Prepare the regular tarball but write it to a temporary file
    tar_fd, tar_path = mkstemp('.tar', BIN + '-')
    bundle(ctx, 'tar', package=tar_path)

    # Compress that file with gzip.
    tar_gz_path = str(OUTPUT_DIR / ('%s.tar.gz' % format_bundle_name(ctx)))
    with os.fdopen(tar_fd, 'rb') as tar_f, \
        gzip.open(tar_gz_path, 'wb') as tar_gz_f:
            shutil.copyfileobj(tar_f, tar_gz_f)

    logging.debug("Compressed release tarball created.")


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
        return False

    ensure_output_dir()
    prepare_release(ctx)

    logging.info("Preparing RedHat package...")
    bundle(ctx, 'rpm', prefix=LINUX_INSTALL_DIR)
    logging.debug("RedHat package created.")


# Shared release stages

def prepare_release(ctx):
    """Prepare release files.

    This includes building the binary in --release mode.
    """
    from tasks import build
    build.all(ctx, release=True)

    if which(ctx, 'strip'):
        ctx.run('strip %s' % (SOURCE_DIR / BIN))
    else:
        logging.warning("'strip' not found, binary will retain debug symbols.")

    # Extra files we want to include.
    cwd = Path.cwd()
    for pattern in EXTRA_FILES:
        for path in cwd.glob(pattern):
            logging.debug("Copying %s file to release directory...",
                          path.relative_to(cwd))
            try:
                shutil.copy(str(path), str(SOURCE_DIR))
            except shutil.Error as e:
                if 'same file' not in str(e):
                    raise


def ensure_output_dir():
    """Ensure that the release directory exists."""
    if OUTPUT_DIR.exists():
        if OUTPUT_DIR.is_dir():
            return
        logging.error("Output path %s already exists but it's not a directory!",
                      OUTPUT_DIR)
        raise Exit(2)

    try:
        logging.info("Creating output directory (%s)...", OUTPUT_DIR)
        OUTPUT_DIR.mkdir(parents=True)
    except IOError as e:
        logging.fatal("Failed to create output directory %s: %s",
                      OUTPUT_DIR, e)
        raise Exit(2)
    else:
        logging.debug("Output directory created.")


# TODO: this should be a class, given that ctx and package_info is shared
# between several functions
def bundle(ctx, target, **flags):
    """Create a release bundle by involving `fpm` with common parameters.

    :param target: Release target, like "deb", "rpm", etc.
    :param flags: Additional flags to be passed to fpm

    :return: Invoke's process object
    """
    package_info = read_package_info()

    # Define the fpm's input.
    flags.update(s='dir', C=str(SOURCE_DIR))

    # Provide package information.
    for key, value in package_info.items():
        flags.setdefault(key, value)
    flags.setdefault('vendor', "<unspecified>")

    # Use Gist SHA of HEAD revision as the --iteration value.
    if 'iteration' not in flags:
        sha = ctx.run('git rev-parse --short HEAD', hide=True).stdout.strip()
        flags['iteration'] = sha

    # Include architecture spec if know.
    # (This env variable should be provided by a CI script).
    arch = os.environ.get('ARCH')
    if arch:
        flags['architecture'] = arch
    else:
        arch = get_architecture(ctx)

    # Use all this info to determine the final release package name:
    # the fpm output.
    package_name = format_bundle_name(ctx, package_info, arch)
    flags['t'] = target
    if 'package' not in flags:
        flags['package'] = str(OUTPUT_DIR / ('%s.%s' % (package_name, target)))

    def format_flag(name, value):
        return '-%s %s' % (name if len(name) == 1 else '-' + name,
                           quote(value))
    fpm_flags = ' '.join(starmap(format_flag, flags.items()))

    # Determine the exact files that comprise the bundle.
    source_files = [BIN]
    for pattern in EXTRA_FILES:
        for path in Path.cwd().glob(pattern):
            try:
                filename = path.relative_to(SOURCE_DIR)
            except ValueError:
                filename = path.relative_to(Path.cwd())
            source_files.append(str(filename))
    fpm_args = ' '.join(map(quote, source_files))

    fpm_cmdline = 'fpm --force --log error %s %s' % (fpm_flags, fpm_args)
    logging.debug("Running %s" % fpm_cmdline)
    return ctx.run(fpm_cmdline)


def read_package_info(cargo_toml=None):
    """Read package info from the [package] section of Cargo.toml.

    :return: Dictionary mapping PACKAGE_FIELDS to their values
    """
    cargo_toml = Path(cargo_toml or Path.cwd() / 'Cargo.toml')
    with cargo_toml.open() as f:
        package_conf = toml.load(f)

    # PACKAGE_INFO defines how to obtain package info from Cargo.toml
    # by providing either a tuple of subsequent keys to follow / transformations
    # to apply; or a verbatim value.
    result = {}
    for field, spec in PACKAGE_INFO.items():
        if isinstance(spec, tuple):
            value = package_conf
            for step in spec:
                value = step(value) if callable(step) else value[step]
        else:
            value = spec
        if value is not None:
            result[field] = value

    return result


def format_bundle_name(ctx, package_info=None, arch=None):
    """Format the name of the bundle, incl. app name, version, etc."""
    package_info = package_info or read_package_info()
    arch = arch or os.environ.get('ARCH') or get_architecture(ctx)
    return '%s-%s-%s' % (package_info['name'], package_info['version'], arch)


def get_architecture(ctx):
    """Build an string describing architecture of the current system.
    :return: Architecture string or 'unknown'
    """
    result = 'unknown'

    if not which(ctx, 'uname'):
        logging.warning('`uname` not found, cannot determine architecture.')
        return result

    uname_os = ctx.run('uname -s', warn=True, hide=True)
    uname_hardware = ctx.run('uname -m', warn=True, hide=True)
    if not (uname_os.ok and uname_hardware.ok):
        logging.error("Running `uname` to obtain architecture info failed!")
        return result

    os_name = uname_os.stdout.strip().lower()  # e.g. 'Linux', 'Darwin'
    hardware_name = uname_hardware.stdout.strip()  # e.g. 'x86_64'
    return '%s-%s' % (hardware_name, os_name)


# Utility functions

def ensure_fpm(ctx):
    """Ensure that fpm is available."""
    if not which(ctx, 'fpm'):
        logging.fatal("fpm not found, aborting.")
        raise Exit(1)


def which(ctx, prog):
    """Runs $ which prog."""
    return ctx.run('which %s' % prog, warn=True, hide=True)
