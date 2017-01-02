#!/bin/bash

# before_deploy: script for Travis on OSX


brew update >/dev/null


# Install the replacement for the missing ssh-askpass
# so that we can deploy using ad-hoc SSH key.
SSH_ASKPASS=/usr/X11R6/bin/ssh-askpass
brew tap theseal/ssh-askpass
brew install ssh-askpass
if [ ! -f "$SSH_ASKPASS" ]; then
    sudo mkdir -p "$(dirname "$SSH_ASKPASS")"
    sudo ln -s /usr/local/bin/ssh-askpass "$SSH_ASKPASS"
fi
