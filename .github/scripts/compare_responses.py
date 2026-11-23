#!/usr/bin/env python3
"""Compare responses from the Docker server and the PHP reference server.

Both implementations create a temporary IGC file with a different (random)
name and the Rust server runs the validator in a random temporary directory, so
file names are normalised before comparing. Output that consists of a wine
crash dump is not compared because it contains process ids and addresses that
differ between runs.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

FIELDS = ("result", "status", "msg", "igc", "ref", "server", "output")

TEMP_FILE = re.compile(
    r"""(?:[A-Za-z]:[\\/][^\s"']*[\\/])?
        (?:flight|iv-[0-9A-Za-z]+)\.igc
    """,
    re.VERBOSE,
)

WINE_CRASH_MARKERS = (
    "Unhandled exception:",
    "Register dump:",
    "Backtrace:",
    "Wine build:",
)

# A validator that crashes under wine may or may not manage to flush its
# IGCVALI result line before the crash, so either server can report a failed
# validation or a service error for the same crash. Treat those as equivalent
# when a crash dump is present on one of the sides.
CRASH_STATUSES = {"ERR_SERVICE", "IGC_FAILED", "IGC_FAILED_NC"}


def has_crash_dump(lines: list[str]) -> bool:
    return any(marker in line for line in lines for marker in WINE_CRASH_MARKERS)


def normalise_output(lines: list[str]) -> list[str] | None:
    """Return the normalised output, or None if it is a wine crash dump."""
    if has_crash_dump(lines):
        return None

    return [TEMP_FILE.sub("<file>", line) for line in lines]


def normalise(response: dict) -> dict:
    normalised = dict(response)
    normalised["output"] = normalise_output(response.get("output") or [])
    # Module names are echoed by ivshell using the spelling it was given. The
    # PHP server passes the name from vali-igc.conf (vali-XMP.exe) while the
    # Rust server resolves it to the file on disk (vali-xmp.exe).
    normalised["msg"] = str(response.get("msg", "")).lower()
    return normalised


def format_value(value: object) -> str:
    if isinstance(value, list):
        return "\n".join(f"            {line}" for line in value) or "            []"
    return f"        {value!r}"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--docker", required=True, type=Path)
    parser.add_argument("--php", required=True, type=Path)
    args = parser.parse_args()

    docker_responses = json.loads(args.docker.read_text())
    php_responses = json.loads(args.php.read_text())

    names = sorted(set(docker_responses) | set(php_responses))
    failed = 0
    skipped_output = 0

    for name in names:
        if name not in docker_responses:
            print(f"MISSING  {name}: no Docker response", file=sys.stderr)
            failed += 1
            continue
        if name not in php_responses:
            print(f"MISSING  {name}: no PHP response", file=sys.stderr)
            failed += 1
            continue

        docker = normalise(docker_responses[name])
        php = normalise(php_responses[name])

        request_errors = {
            side: response["_request_error"]
            for side, response in (("docker", docker_responses[name]), ("php", php_responses[name]))
            if "_request_error" in response
        }
        if request_errors:
            failed += 1
            print(f"ERROR    {name}")
            for side, error in request_errors.items():
                print(f"    {side}: {error}")
            continue

        docker_crash = has_crash_dump(docker_responses[name].get("output") or [])
        php_crash = has_crash_dump(php_responses[name].get("output") or [])
        if (
            (docker_crash or php_crash)
            and docker["status"] != php["status"]
            and {docker["status"], php["status"]} <= CRASH_STATUSES
        ):
            skipped_output += 1
            print(f"ok       {name} (validator crashed under wine)")
            continue

        if docker["output"] is None or php["output"] is None:
            skipped_output += 1
            docker["output"] = None
            php["output"] = None

        differences = [
            field
            for field in FIELDS
            if field in docker or field in php
            if docker.get(field) != php.get(field)
        ]

        if differences:
            failed += 1
            print(f"DIFF     {name}")
            for field in differences:
                print(f"    {field}:")
                print(f"        docker: {format_value(docker.get(field))}")
                print(f"        php:    {format_value(php.get(field))}")
        else:
            print(f"ok       {name}")

    print()
    print(f"Compared {len(names)} sample files: {len(names) - failed} match, {failed} differ")
    if skipped_output:
        print(f"Skipped output comparison for {skipped_output} file(s) with crash dumps")

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
