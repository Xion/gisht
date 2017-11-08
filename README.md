# gisht

Gists in the shell

[![Build Status](https://img.shields.io/travis/Xion/gisht.svg)](https://travis-ci.org/Xion/gisht)
[![License](https://img.shields.io/github/license/Xion/gisht.svg)](https://github.com/Xion/gisht/blob/master/LICENSE)

With *gisht*, you can run scripts published as GitHub (or other) gists with a single command::

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
        run      Run the specified gist [aliases: exec]
        which    Output the path to gist's binary
        print    Print the source code of gist's binary [aliases: cat]
        open     Open the gist's webpage [aliases: show]
        info     Display summary information about the gist [aliases: stat]
        help     Prints this message or the help of the given subcommand(s)
    
    Hint: `gisht run GIST` can be shortened to just `gisht GIST`.
    If you want to pass arguments, put them after `--` (two dashes), like this:
    
    	gisht Octocat/greet -- "Hello world" --cheerful

## Installation

[Binaries are available](https://github.com/Xion/gisht/releases) for Linux and Mac.

If you use **Mac OS X**, `gisht` can be installed with **Homebrew**:

    brew tap Xion/gisht https://github.com/Xion/gisht.git
    brew install gisht

Windows binaries coming soon.

## Development

`gisht` is written in Rust. Besides the [Rust toolchain](http://rustup.sh), build requirements include:

* cmake 2.8.11 or higher (for compiling libgit2)
* OpenSSL 1.1 (for hyper)
  * on Linux, it likely means `libssl1.1`, `libssl-dev`, and/or equivalent package(s) must be installed
  * on OSX, besides the relevant package, it may also require adjusting some environment variables
  * (Windows unknown)
* Some Linux setups may require installing of `libssh-dev` and `pkg-config`.

Additionally, the Python-based [Invoke](http://pyinvoke.org) task runner is used for automation.
It is recommended you install it inside a Python virtualenv. e.g.:

    $ virtualenv ~/venv/gisht && source ~/venv/gisht/bin/activate
    $ pip install -r -requirements-dev.txt

Then you can use:

    $ inv

to build the binary and run tests.
