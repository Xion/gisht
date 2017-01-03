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
    if gem uninstall bundler -v '>1.12.5' --force ; then
        gem install bundler -v 1.12.5 --no-rdoc --no-ri --no-document --quiet
    else
        echo "bundler is already in a working version, not installing 1.12.5"
    fi
fi

# Install fpm.
$_sudo gem install fpm


#
# Building release packages
#

inv release --platform "$TRAVIS_OS_NAME"
