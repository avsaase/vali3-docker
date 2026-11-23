#!/usr/bin/env python3
"""Send every IGC sample in vali3/downloads to a vali server and save the JSON."""

from __future__ import annotations

import argparse
import http.client
import json
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

REQUEST_ERRORS = (OSError, json.JSONDecodeError, http.client.HTTPException)


def probe_status(url: str, timeout: float) -> int | None:
    """Return the HTTP status, or None when no response was received."""
    request = urllib.request.Request(url, method="GET")
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status
    except urllib.error.HTTPError as error:
        return error.code
    except REQUEST_ERRORS:
        return None


def wait_for_server(url: str, timeout: float, require_success: bool) -> None:
    parsed = urllib.parse.urlparse(url)
    host = parsed.hostname or "127.0.0.1"
    port = parsed.port or (443 if parsed.scheme == "https" else 80)

    print(f"Waiting for {url} ...", file=sys.stderr, flush=True)

    deadline = time.monotonic() + timeout
    last_status: int | None = None

    while time.monotonic() < deadline:
        last_status = probe_status(
            url,
            min(5.0, max(1.0, deadline - time.monotonic())),
        )

        if last_status is not None:
            ready = (
                200 <= last_status < 300
                if require_success
                else last_status < 500
            )
            if ready:
                print(f"Ready: {url}", file=sys.stderr, flush=True)
                return

            # A 404 on an explicit readiness endpoint will not fix itself.
            if require_success and last_status == 404:
                raise SystemExit(
                    f"{url} returned HTTP 404. The running image may be "
                    "outdated or does not expose this readiness endpoint."
                )

        time.sleep(1)

    detail = (
        f"last HTTP status: {last_status}"
        if last_status is not None
        else "no response"
    )
    raise SystemExit(
        f"server at {host}:{port} was not ready within {timeout:.0f}s "
        f"({url}, {detail})"
    )


def post_igc(url: str, path: Path, timeout: float) -> dict:
    boundary = f"----vali{uuid.uuid4().hex}"
    body = bytearray()

    body += (
        f'--{boundary}\r\n'
        'Content-Disposition: form-data; name="out"\r\n'
        "\r\n"
        "vali\r\n"
    ).encode()
    body += (
        f'--{boundary}\r\n'
        f'Content-Disposition: form-data; name="igcfile"; filename="{path.name}"\r\n'
        "Content-Type: application/octet-stream\r\n"
        "\r\n"
    ).encode()
    body += path.read_bytes()
    body += f"\r\n--{boundary}--\r\n".encode()

    request = urllib.request.Request(
        url,
        data=bytes(body),
        method="POST",
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
    )

    with urllib.request.urlopen(request, timeout=timeout) as response:
        return json.loads(response.read().decode("utf-8"))


def post_igc_with_retry(url: str, path: Path, timeout: float) -> dict:
    # wine occasionally fails to start a validator under load. Retry a couple
    # of times without slowing down the run; persistent differences are still
    # reported.
    last_response: dict | None = None
    last_error: Exception | None = None

    for _ in range(3):
        try:
            response = post_igc(url, path, timeout)
        except REQUEST_ERRORS as error:
            last_error = error
        else:
            last_response = response
            if response.get("status") != "ERR_SERVICE":
                return response

    if last_response is not None:
        return last_response

    assert last_error is not None
    raise last_error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", required=True)
    parser.add_argument(
        "--ready-url",
        help="readiness endpoint; must return 2xx. Defaults to probing --base-url",
    )
    parser.add_argument("--downloads", default="vali3/downloads", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--timeout", default=180.0, type=float)
    parser.add_argument(
        "--wait",
        default=120.0,
        type=float,
        help="seconds to wait for the server readiness probe to succeed",
    )
    args = parser.parse_args()

    files = sorted(args.downloads.glob("*.igc"))
    if not files:
        raise SystemExit(f"no .igc files found in {args.downloads}")

    wait_for_server(
        args.ready_url or args.base_url,
        args.wait,
        require_success=args.ready_url is not None,
    )

    results: dict[str, object] = {}
    failures = 0

    for index, path in enumerate(files, 1):
        try:
            results[path.name] = post_igc_with_retry(args.base_url, path, args.timeout)
            print(f"[{index:3}/{len(files)}] {path.name}", flush=True)
        except REQUEST_ERRORS as error:
            failures += 1
            results[path.name] = {"_request_error": str(error)}
            print(f"[{index:3}/{len(files)}] {path.name}: {error}", file=sys.stderr)

    args.output.write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")
    print(f"Wrote {len(results)} responses to {args.output}")

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
