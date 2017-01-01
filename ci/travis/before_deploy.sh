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

    # Force bundler 1.12.5 because version 1.13 has issues,
    # see https://github.com/fastlane/fastlane/issues/6065#issuecomment-246044617
    gem uninstall bundler -v '>1.12.5' --force || echo "bundler >1.12.5 is not installed"
    gem install bundler -v 1.12.5 --no-rdoc --no-ri --no-document --quiet
fi

# Install fpm.
$_sudo gem install fpm


#
# Building release packages
#

if [[ "$TRAVIS_OS_NAME" == "osx" ]]; then
    inv release --platform darwin
else
    inv release --platform linux
fi
