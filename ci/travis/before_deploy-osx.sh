#!/bin/bash

# before_deploy: script for Travis on OSX


brew update >/dev/null

# Install the replacement for the missing ssh-askpass
# so that we can deploy using ad-hoc SSH key.
brew tap theseal/ssh-askpass
brew install ssh-askpass
sudo mkdir -p /usr/X11R6/bin
sudo ln -s /usr/local/bin/ssh-askpass /usr/X11R6/bin/ssh-askpass
