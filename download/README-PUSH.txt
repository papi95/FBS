GitHub push

The page at / has a "Push to GitHub" panel. It starts the sign-in automatically,
shows a device code, and finishes the push (plus a release with the Windows ZIP)
once the code is approved.

The work is done by scripts/github-push.sh, which the preview server runs in the
background. Progress is written to download/github.json so the page can poll it.

This design exists because the workspace sleeps between chat turns: any login
started inside a turn dies before the code can be read. Running it from the
preview page keeps the process alive while the user is looking at the code.
