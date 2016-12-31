#!/bin/sh

# before_install: script for Travis on OSX


brew update >/dev/null

# Install OpenSSL.
# (incantations taken from https://github.com/sfackler/rust-openssl/issues/255)
brew install openssl
export OPENSSL_INCLUDE_DIR=`brew --prefix openssl`/include
export OPENSSL_LIB_DIR=`brew --prefix openssl`/lib
export DEP_OPENSSL_INCLUDE=`brew --prefix openssl`/include

# Install Python and prepare virtualenv for Invoke tasks' dependencies.
# (This has to be done in before_install: section as per this comment:
#  https://github.com/travis-ci/travis-ci/issues/2312#issuecomment-247206351)
brew install python
virtualenv $TRAVIS_BUILD_DIR/.venv
source $TRAVIS_BUILD_DIR/.venv/bin/activate

# Install latest stable Rust via rustup.
# This is necessary because we're running on OSX as language:generic
# (due to Python hack above), so we don't have Rust handy.
brew install rust
rustc --version
cargo --version
# TODO: use rustup and test on multiple Rust versions;
# this would likely make it necessary to move the installation to before_script:,
# as per: https://docs.travis-ci.com/user/installing-dependencies/#Installing-Projects-from-Source
