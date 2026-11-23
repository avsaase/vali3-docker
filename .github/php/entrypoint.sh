#!/bin/sh
set -eu

# The PHP CI image is built without a Wine prefix so that the image build works
# on Docker versions that block the syscalls Wine needs. Initialize it here.
if [ ! -f "$WINEPREFIX/system.reg" ]; then
    wineboot --init
    wineserver --wait
fi

wineserver --persistent

exec php \
    -d max_execution_time=0 \
    -d post_max_size=8M \
    -d upload_max_filesize=8M \
    -S 0.0.0.0:8081 \
    -t /app/vali3
