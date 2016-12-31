"""
Release tasks.
"""
from pathlib import Path

from invoke import Collection, task


@task(name="all")
def all_(ctx):
    """Create all release packages."""
    from tasks.release.fpm import deb, rpm, tar

    # Generic.
    tar(ctx)

    # Linux.
    deb(ctx)
    rpm(ctx)


# Task setup

ns = Collection()
ns.add_task(all_, default=True)

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
