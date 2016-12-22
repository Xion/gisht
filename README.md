# gisht

Gists in the shell

[![Build Status](https://img.shields.io/travis/Xion/gisht.svg)](https://travis-ci.org/Xion/gisht)
[![License](https://img.shields.io/github/license/Xion/gisht.svg)](https://github.com/Xion/gisht/blob/master/LICENSE)

With *gisht*, you can run scripts published as GitHub gists with a single command::

    gisht Xion/git-today

Behind the scenes, *gisht* will fetch the gist, cache it locally, and run its code.
Magic!

## Usage

    gisht [OPTIONS] [SUBCOMMAND]

    OPTIONS:
        -c, --cached     Operate only on gists available locally
        -f, --fetch      Always fetch the gist from a remote host
        -v, --verbose    Increase logging verbosity
        -q, --quiet      Decrease logging verbosity
        -H, --help       Prints help information
        -V, --version    Prints version information

    SUBCOMMANDS:
        run      Run the specified gist
        which    Output the path to gist's binary
        print    Print the source code of gist's binary
        open     Open the gist's webpage
        info     Display summary information about the gist
        help     Prints this message or the help of the given subcommand(s)

    Hint: `gisht run GIST` can be shortened to just `gisht GIST`.
    If you want to pass arguments, put them after `--` (two dashes), like this:

    	gisht Octocat/greet -- "Hello world" --cheerful

## Development

`gisht` is written in Rust. Besides the [Rust toolchain](http://rustup.sh), build requirements include:

* cmake 2.8.11 or higher (for compiling libgit2)
* OpenSSL (for hyper) -- likely only a problem on OSX

Additionally, the Python-based [Invoke](http://pyinvoke.org) task runner is used for automation.
It is recommended you install it inside a Python virtualenv. e.g.:

    $ virtualenv ~/venv/gisht && source ~/venv/gisht/bin/activate
    $ pip install -r -requirements-dev.txt

Then you can use:

    $ inv

to build the binary and run tests.
