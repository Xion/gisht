# gisht

Gists in the shell

## Usage

TBD

## Development

`gisht` is written in Rust. Besides the [Rust toolchain](http://rustup.sh), build requirements include:

* cmake (for compiling the libgit2)

Additionally, the Python-based [Invoke](http://pyinvoke.org) task runner is used for automation.
It is recommended you install it inside a Python virtualenv. e.g.:

    $ virtualenv ~/venv/rush && source ~/venv/rush/bin/activate
    $ pip install -r -requirements-dev.txt

Then you can use:

    $ inv

to build the binary and run tests.
