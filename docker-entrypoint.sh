#!/bin/sh
set -eu

# Initialize the Wine prefix on first start if the image was built with
# INIT_WINE=false (see the Dockerfile), for example on Docker 29.4.2 whose
# seccomp profile blocks the socket syscalls Wine needs.
if [ ! -f "$WINEPREFIX/system.reg" ]; then
    wineboot --init
    wineserver --wait
fi

# Keep a wineserver alive for the lifetime of the container. Starting it once up
# front avoids paying the Wine startup cost for every validation request.
wineserver --persistent

exec /usr/local/bin/vali3-docker "$@"
