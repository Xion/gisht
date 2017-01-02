#!/bin/bash

# after_deploy: script for Travis on OS X
# Currently, it is used to commit & push the autogenerated Homebrew formula

# Based heavily on https://gist.github.com/domenic/ec8b0fc8ab45f39403dd

set -e

FORMULA=gisht.rb
SRC_DIR=release
DEST_DIR=pkg/homebrew
DEPLOY_KEY_ENC=ci/travis/deploy_key.enc

# Git repo information.
REPO=`git config remote.origin.url`
SSH_REPO=${REPO/https:\/\/github.com\//git@github.com:}
SHA=`git rev-parse --verify HEAD`


#
# Deploying Homebrew formula
#

# Copy the generated formula (done in before_deploy: step)
# to the destination directory.
mkdir -p "$DEST_DIR"
cp -f -v "$SRC_DIR/$FORMULA" "$DEST_DIR/$FORMULA"

# If this caused no changes, abort at this point.
git add --intent-to-add "$DEST_DIR/$FORMULA"
if [ -z `git diff --exit-code` ]; then
    echo "Homebrew formula unchanged, exiting."
    exit 0
fi

# Commit the formula.
git config user.name "Travis CI"
git config user.email "$COMMIT_AUTHOR_EMAIL"
git add "$DEST_DIR/$FORMULA"
git commit -m "Update Homebrew formula to version $TRAVIS_TAG (sha: $SHA)"

# Get the deploy key by using Travis's stored variables to decrypt deploy_key.enc
ENCRYPTED_KEY_VAR="encrypted_${DEPLOY_ENC_LABEL}_key"
ENCRYPTED_IV_VAR="encrypted_${DEPLOY_ENC_LABEL}_iv"
ENCRYPTED_KEY=${!ENCRYPTED_KEY_VAR}
ENCRYPTED_IV=${!ENCRYPTED_IV_VAR}
openssl aes-256-cbc -K $ENCRYPTED_KEY -iv $ENCRYPTED_IV -in $DEPLOY_KEY_ENC -out deploy_key -d
chmod 600 deploy_key
eval `ssh-agent -s`
ssh-add deploy_key

# Push the changes.
git push $SSH_REPO $TRAVIS_BRANCH