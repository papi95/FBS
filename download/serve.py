#!/usr/bin/env python3
"""Serve the FBS download page and run the GitHub push on request.

Why a custom server: the GitHub sign-in needs a process that outlives a chat
turn (the workspace sleeps between turns, which kept killing the device code
before it could be read). Here the code is generated on the same page the user
is already looking at, and the push runs in the background while the page polls
for progress.
"""
import http.server
import json
import os
import re
import socketserver
import subprocess
import sys

PROJECT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SERVE_DIR = os.path.join(PROJECT, "download")
STATE = os.path.join(SERVE_DIR, "github.json")
PUSH = os.path.join(PROJECT, "scripts", "github-push.sh")
PORT = int(sys.argv[1]) if len(sys.argv) > 1 else int(os.environ.get("PORT", 3000))

BUSY = {"starting", "signing_in", "waiting_code", "creating", "pushing"}


def read_state():
    try:
        with open(STATE) as fh:
            return json.load(fh)
    except Exception:
        return {"state": "idle"}


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=SERVE_DIR, **kwargs)

    def log_message(self, *args):
        pass

    def do_POST(self):
        if self.path.split("?")[0] != "/push":
            self.send_error(404)
            return
        if read_state().get("state") in BUSY:
            self._json(read_state())
            return
        self._json({"state": "starting"})
        subprocess.Popen(
            ["sh", PUSH],
            cwd=PROJECT,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )

    def do_GET(self):
        path = self.path.split("?")[0]
        if path == "/github.json":
            self._json(read_state())
            return
        # Only plain names: no traversal, no dotfiles.
        if not re.fullmatch(r"/[A-Za-z0-9._-]*", path):
            self.send_error(404)
            return
        super().do_GET()

    def _json(self, obj):
        body = json.dumps(obj).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


if __name__ == "__main__":
    with Server(("0.0.0.0", PORT), Handler) as httpd:
        httpd.serve_forever()
