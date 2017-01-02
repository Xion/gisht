"""
Tasks for preparing the Homebrew formula describing gisht package for OSX.
"""
import hashlib
import inspect
import logging
from pathlib import Path
import string

from invoke import task
import toml

from tasks import BIN
from tasks.release import RELEASE_DIR


# Template for the formula file.
#
# Note that this uses the (relatively uncommon) Python string templating
# with $PLACERHOLDERS_LIKE_THIS.
FORMULA_TEMPLATE = """

class $name < Formula
  version '$version'
  desc "$description"
  homepage "$homepage"
  url "$url"
  sha256 "$sha"

  def install
    bin.install "$bin"
    # TODO: generate man pages
    # man1.install "$bin.1"
  end
end

""".strip()

# Template for the URL to the compiled bundle.
# (This is also a Python template string by #{those_placeholders} are Ruby ones
#  and are not touched by the string.Template.substitute).
URL_TEMPLATE = ("$repository/releases/download/#{version}/"
                "$name-#{version}-x86_64-apple-darwin.tar.gz")


@task
def brew(ctx):
    """Create the Homebrew formula."""
    from tasks.release.fpm import tar_gz

    logging.info("Generating Homebrew formula...")

    variables = read_package_info()
    variables['name'] = variables['name'].capitalize()
    variables['bin'] = BIN

    # Create the .tar.gz bundle and calculate its SHA256.
    bundle = tar_gz(ctx)
    sha256 = hex_digest(bundle, 'sha256')
    variables['sha'] = sha256

    # Format the URL where the bundle will have been upload to.
    url = render(URL_TEMPLATE, **variables)
    variables['url'] = url

    # Render the formula and write it to file.
    formula = render(FORMULA_TEMPLATE, **variables)
    formula_file = RELEASE_DIR / ('%s.rb' % variables['name'])
    with formula_file.open('w', encoding='utf-8') as f:
        f.write(formula)

    logging.debug("Homebrew formula generaed.")



# Utility functions

def hex_digest(filename, hasher, buf_size=65536):
    """Produce a digest of given file via given hashlib hasher."""
    if isinstance(hasher, str):
        hasher = hashlib.new(hasher)
    elif inspect.isbuiltin(hasher):
        hasher = hasher()

    path = Path(filename)
    with path.open('rb') as f:
        while True:
            buf = f.read(buf_size)
            if not buf:
                break
            hasher.update(buf)

    return hasher.hexdigest()


def read_package_info(cargo_toml=None):
    """Read the content of [package] section from Cargo.toml.
    :return: [package] section as dictionary
    """
    cargo_toml = Path(cargo_toml or Path.cwd() / 'Cargo.toml')
    with cargo_toml.open() as f:
        manifest = toml.load(f)
    return manifest['package']


def render(template, **subst):
    """Render a string.Template with given substitutions."""
    return string.Template(template).substitute(subst)
