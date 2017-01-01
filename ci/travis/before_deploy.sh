#!/bin/sh

# before_deploy: script for Travis


_sudo=""
if [[ "$TRAVIS_OS_NAME" == "osx" ]]; then
    _sudo="sudo"
fi


#
# Release dependencies
#

# Install Ruby on OSX. (On Linux it is already installed in travis.yml)
if [[ "$TRAVIS_OS_NAME" == "osx" ]]; then
    brew update >/dev/null
    brew install ruby
fi

# Install fpm.
# (Pick specific version because 1.8.0+ seems to be borked, at least on OSX)
$_sudo gem install fpm -v 1.6.3


#
# Building release packages
#

if [[ "$TRAVIS_OS_NAME" == "osx" ]]; then
    inv release --platform darwin
else
    inv release --platform linux
fi
