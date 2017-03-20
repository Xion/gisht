"""
Automation tasks for preparing release bundles with the gisht binary.

This essentially a wrapper script around fpm (https://github.com/jordansissel/fpm).
Requires fpm to be installed first, which may in turn require Ruby with C headers.
Refer to fpm's README for installation instructions.
"""
import collections
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
import tempfile

from invoke import task
from invoke.exceptions import Exit
import toml

from tasks import BIN
from tasks.release import RELEASE_DIR


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
    Bundler(ctx, 'tar').build()
    logging.debug("Release tarball created.")


@task
def tar_gz(ctx):
    """Create a gzip-compressed release tarball."""
    ensure_fpm(ctx)
    ensure_output_dir()
    prepare_release(ctx)

    logging.info("Preparing compressed release tarball...")
    bundler = Bundler(ctx, 'tar')

    # Prepare the regular tarball but write it to a temporary file.
    tar_path = tempfile.mktemp('.tar', BIN + '-')
    bundler.build(package=tar_path)

    # Compress that file with gzip.
    tar_gz_path = str(OUTPUT_DIR / ('%s.tar.gz' % bundler.bundle_name))
    with open(tar_path, 'rb') as tar_f:
        with gzip.open(tar_gz_path, 'wb') as tar_gz_f:
            shutil.copyfileobj(tar_f, tar_gz_f)

    logging.debug("Compressed release tarball created.")
    return tar_gz_path


@task
def deb(ctx):
    """Create the release package for Debian (.deb)."""
    ensure_fpm(ctx)
    ensure_output_dir()
    prepare_release(ctx)

    logging.info("Preparing Debian package...")
    Bundler(ctx, 'deb').build(prefix=LINUX_INSTALL_DIR)
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
    Bundler(ctx, 'rpm').build(prefix=LINUX_INSTALL_DIR)
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


class Bundler(object):
    """Creates a release bundle by invoking `fpm`."""

    UNKNOWN_ARCH = 'unknown'

    def __init__(self, ctx, target):
        """Constructor.

        :param ctx: Invoke's task context
        :param target: Release target, like "deb", "rpm", "tar", etc.
        """
        self._ctx = ctx
        self._target = target

        # Include architecture spec if known.
        # (This env variable should be provided by a CI script).
        try:
            self._arch = os.environ['ARCH']
        except KeyError:
            self._arch = self._get_architecture()

        self._package = PackageInfo()

    def _get_architecture(self):
        """Build an string describing architecture of the current system.
        :return: Architecture string or 'unknown'
        """
        if not self._which('uname'):
            logging.warning('`uname` not found, cannot determine architecture.')
            return self.UNKNOWN_ARCH

        uname_os = self._run('uname -s', warn=True, hide=True)
        uname_hardware = self._run('uname -m', warn=True, hide=True)
        if not (uname_os.ok and uname_hardware.ok):
            logging.error("Running `uname` to obtain architecture info failed!")
            return self.UNKNOWN_ARCH

        os_name = uname_os.stdout.strip().lower()  # e.g. 'Linux', 'Darwin'
        hardware_name = uname_hardware.stdout.strip()  # e.g. 'x86_64'
        return '%s-%s' % (hardware_name, os_name)

    @property
    def bundle_name(self):
        """Name of the release bundle. This is used as a filename."""
        return '%s-%s-%s' % (
            self._package.name, self._package.version, self._arch)

    def build(self, **flags):
        """Call fpm to create the release bundle.

        :param flags: Additional flags to be passed to fpm

        :return: Invoke's process object
        """
        # Define the fpm's input and output.
        flags.update(s='dir', C=str(SOURCE_DIR))
        flags.update(t=self._target)
        flags.setdefault('package', str(
            OUTPUT_DIR / ('%s.%s' % (self.bundle_name, self._target))))

        # Provide package information.
        for key, value in self._package.items():
            flags.setdefault(key, value)
        flags.setdefault('vendor', "<unspecified>")
        flags.setdefault('architecture', self._arch)

        # Use Gist SHA of HEAD revision as the --iteration value.
        if 'iteration' not in flags:
            sha = self._run(
                'git rev-parse --short HEAD', hide=True).stdout.strip()
            flags['iteration'] = sha

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
        return self._run(fpm_cmdline)

    # Utility methods

    def _run(self, *args, **kwargs):
        """Helper method to run subprocesses via Invoke's context."""
        return self._ctx.run(*args, **kwargs)

    def _which(self, prog):
        """Check if given program is available."""
        return which(self._ctx, prog)


class PackageInfo(collections.Mapping):
    """Information about a Rust package, as read from its Cargo.toml."""

    # Package fields dictionary,
    # specifying how they are retrieved from Cargo.toml.
    #
    # Fields that map to tuples are retrieved from the corresponding field path
    # in Cargo.toml (and optionally undergo transformations through functions).
    FIELDS = dict(
        name=('package', 'name'),
        version=('package', 'version'),
        description=('package', 'description'),
        url=('package', 'homepage'),
        license=('package', 'license'),
        maintainer=('package', 'authors', 0,
                    lambda v: v[:v.find('<') - 1] if '<' in v else v),
    )

    def __init__(self, cargo_toml=None):
        """Constructor.

        :param cargo_toml: Path to Cargo.toml of the package.
                           By default, it will be the Cargo.toml in the
                           current directory.
        """
        cargo_toml = Path(cargo_toml or Path.cwd() / 'Cargo.toml')
        self._cargo_toml = cargo_toml

        with cargo_toml.open() as f:
           self._manifest = toml.load(f)

    def _get_field_value(self, field):
        """Retrieve a value of the field from Cargo.toml package manifest."""
        try:
            spec = self.FIELDS[field]
        except KeyError:
            raise ValueError(field)

        # If the spec is a tuple, go down the TOML path, potentially applying
        # some tranformations along the way.
        if isinstance(spec, tuple):
            value = self._manifest
            for step in spec:
                value = step(value) if callable(step) else value[step]
        else:
            value = spec

        # Cache the value on the object for subsequent retrieval.
        setattr(self, field, value)
        return value

    def __getitem__(self, key):
        """collections.Mapping interface."""
        try:
            return self._get_field_value(key)
        except ValueError:
            raise KeyError(key)

    def __iter__(self):
        """collections.Mapping interface."""
        return (k for k in self.FIELDS.keys())

    def __len__(self):
        """collections.Mapping interface."""
        return len(self.FIELDS)

    def __getattr__(self, field):
        """Convenience interface for getting package fields as attributes."""
        try:
            return self._get_field_value(field)
        except ValueError:
            raise AttributeError(field)


# Utility functions

def ensure_fpm(ctx):
    """Ensure that fpm is available."""
    if not which(ctx, 'fpm'):
        logging.fatal("fpm not found, aborting.")
        raise Exit(1)


def which(ctx, prog):
    """Runs $ which prog."""
    return ctx.run('which %s' % prog, warn=True, hide=True)
