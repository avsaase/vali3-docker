#!/bin/sh
set -eu

# Keep a wineserver alive for the lifetime of the container. Starting it once up
# front avoids paying the Wine startup cost for every validation request.
wineserver --persistent

exec /usr/local/bin/vali3-docker "$@"
