#!/bin/sh
set -eu

# Initialize the Wine prefix on first start. Doing this at container start
# instead of at image build time keeps the image buildable on Docker versions
# whose seccomp profile blocks the socket syscalls Wine needs (e.g. 29.4.2).
if [ ! -f "$WINEPREFIX/system.reg" ]; then
    wineboot --init
    wineserver --wait
fi

# Keep a wineserver alive for the lifetime of the container. Starting it once up
# front avoids paying the Wine startup cost for every validation request.
wineserver --persistent

exec /usr/local/bin/vali3-docker "$@"
