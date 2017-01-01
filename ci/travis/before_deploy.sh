#!/bin/sh

# before_deploy: script for Travis


_sudo=""
if [[ "$TRAVIS_OS_NAME" == "osx" ]]; then
    _sudo="sudo"
fi


#
# Release dependencies
#

# Install Ruby.
if [[ "$TRAVIS_OS_NAME" == "osx" ]]; then
    brew update >/dev/null
    brew install ruby
else
    apt-get install ruby ruby-dev
fi

# Install fpm.
_sudo gem install fpm


#
# Building release packages
#

if [[ "$TRAVIS_OS_NAME" == "osx" ]]; then
    inv release.tar_gz
else
    inv release
fi
