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


@task(help={
    'release': "Whether to build the binary in release mode.",
})
def build(ctx, release=False):
    """Build the project."""
    flags = ['--release'] if release else []
    cargo(ctx, 'build', *flags, wait=False)


@task
def clean(ctx):
    """Clean all build artifacts."""
    cargo(ctx, 'clean', wait=False)


# Utility functions

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


# This precondition makes it easier to localize files needed by tasks.
if not os.path.exists(os.path.join(os.getcwd(), '.gitignore')):
    logging.fatal(
        "Automation tasks can only be invoked from project's root directory!")
    sys.exit(1)
