#!/bin/sh
# Sign in to GitHub, create the repo, push, and attach the Windows build as a
# release asset. Writes progress to download/github.json so the download page
# can show the live state.
#
# Design note: this runs as part of the preview service, because the workspace
# sleeps between turns and any login started in a turn dies before the user can
# read the device code. On the preview page the code is fresh and the user is
# already looking at it.
set -u
cd "$(dirname "$0")/.." || exit 1

OUT=download/github.json
REPO=fbs
VERSION=v0.1.0

state() {
    printf '{"state":"%s","code":"%s","url":"%s","msg":"%s"}\n' "$1" "$2" "$3" "$4" > "$OUT.tmp"
    mv "$OUT.tmp" "$OUT"
}

command -v gh >/dev/null 2>&1 || {
    state error "" "" "GitHub CLI is missing on this worker; ask me to reinstall it."
    exit 1
}

if ! gh auth status >/dev/null 2>&1; then
    state signing_in "" "https://github.com/login/device" "Waiting for you to approve"

    rm -f .ghcode
    printf 'Y\n' | gh auth login --hostname github.com --git-protocol https --web -s repo \
        > .ghcode 2>&1 &
    LOGIN_PID=$!

    code=""
    i=0
    while [ "$i" -lt 120 ]; do
        code=$(grep -o 'code: [A-Za-z0-9-]*' .ghcode 2>/dev/null | head -1 | awk '{print $2}')
        [ -n "$code" ] && break
        sleep 0.5
        i=$((i + 1))
    done

    if [ -z "$code" ]; then
        state error "" "" "Could not start the GitHub sign-in. Ask me to retry."
        exit 1
    fi
    state waiting_code "$code" "https://github.com/login/device" "Enter this code on GitHub"

    # Poll for the credential file the CLI writes on success.
    i=0
    while [ "$i" -lt 400 ]; do
        gh auth status >/dev/null 2>&1 && break
        kill -0 "$LOGIN_PID" 2>/dev/null || break
        sleep 1
        i=$((i + 1))
    done

    if ! gh auth status >/dev/null 2>&1; then
        state expired "$code" "https://github.com/login/device" "That code expired. Reload this page for a new one."
        exit 1
    fi
fi

# Let git use gh's stored credential for this push; no token is ever stored in
# the repository or passed through a shell argument.
gh auth setup-git --hostname github.com >/dev/null 2>&1

user=$(gh api user -q .login 2>/dev/null)
if [ -z "$user" ]; then
    state error "" "" "Signed in, but could not read the GitHub account name."
    exit 1
fi

if gh repo view "$user/$REPO" >/dev/null 2>&1; then
    state pushing "" "https://github.com/$user/$REPO" "Pushing to the existing repository"
else
    state creating "" "https://github.com/new" "Creating $user/$REPO"
    gh repo create "$user/$REPO" --public \
        --description "FBS — Free Broadcast Studio: Rust live production app (OBS-style workflow)" \
        >/dev/null 2>&1
fi

git remote remove origin >/dev/null 2>&1
git remote add origin "https://github.com/$user/$REPO.git"
git config user.name "FBS"
git config user.email "fbs@localhost"

# gh auth setup-git installs a credential helper, so this needs no token in the URL.
if ! git push -u origin main >.ghpush.log 2>&1; then
    state error "" "https://github.com/$user/$REPO" "Push failed: $(tail -1 .ghpush.log)"
    exit 1
fi

state pushing "" "https://github.com/$user/$REPO" "Publishing the Windows build"

notes="Windows test build of FBS. Download fbs-windows.zip, extract it anywhere, then run gpu_report.exe first and fbs.exe second. Nothing installs.

The build is unsigned, so SmartScreen will ask you to choose More info, then Run anyway."

if gh release view "$VERSION" --repo "$user/$REPO" >/dev/null 2>&1; then
    gh release upload "$VERSION" fbs-windows.zip --clobber --repo "$user/$REPO" >/dev/null 2>&1
else
    gh release create "$VERSION" fbs-windows.zip \
        --repo "$user/$REPO" \
        --title "FBS $VERSION — Windows test build" \
        --notes "$notes" >/dev/null 2>&1
fi

state done "" "https://github.com/$user/$REPO" "Pushed to GitHub"
