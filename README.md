# gisht

Gists in the shell

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
