"""
Tasks for preparing release bundles for various platform.

Actual _deployment_ of those releases (e.g. as GitHub Releases)
is NOT handled here.
"""
import logging
from pathlib import Path
import shutil
import sys

from invoke import Collection, task


RELEASE_DIR = Path.cwd() / 'release'


@task(name="all", help={
    'platform': "Platform to build the releases for (as per sys.platform)",
})
def all_(ctx, platform=None):
    """Create all release packages."""
    from tasks.release.fpm import deb, rpm, tar_gz
    from tasks.release.homebrew import brew

    platform = (platform or '').lower().strip() or sys.platform
    if platform in ('osx', 'mac', 'macosx'):
        platform = 'darwin'
    is_platform = lambda p: platform == 'all' or platform.startswith(p)

    built_any = False

    # Linux
    if is_platform('linux'):
        deb(ctx)
        rpm(ctx)
        built_any = True

    # OS X
    if is_platform('darwin'):
        brew(ctx)
        built_any = True

    # Fallback / miscellaneous release bundles
    if not built_any or platform == 'all':
        # Generic.
        # tar(ctx)  # disabled
        tar_gz(ctx)


@task
def clean(ctx):
    """Clean up release artifacts."""
    if RELEASE_DIR.is_dir():
        try:
            shutil.rmtree(str(RELEASE_DIR))
        except (OSError, shutil.Error) as e:
            logging.warning("Error while cleaning release dir: %s", e)
        else:
            RELEASE_DIR.mkdir()


# Task setup

ns = Collection()
ns.add_task(all_, default=True)
ns.add_task(clean)

# Import every task from submodules directly into the root task namespace
# (without creating silly qualified names like `release.fpm.deb`
# instead of just `release.deb`).
submodules = [f.stem for f in Path(__file__).parent.glob('*.py')
              if f.name != '__init__.py']
this_module = __import__('tasks.release', fromlist=submodules)
for mod_name in submodules:
    module = getattr(this_module, mod_name)
    collection = Collection.from_module(module)
    for task_name in collection.task_names:
        task = getattr(module, task_name)
        ns.add_task(task)
