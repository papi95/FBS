#!/bin/sh
# One-time GitHub device login. Writes the code to .ghlogin.log and waits.
cd /home/ubuntu/project || exit 1
printf 'Y\n' | gh auth login --hostname github.com --git-protocol https --web -s repo 2>&1 | tee .ghlogin.log
