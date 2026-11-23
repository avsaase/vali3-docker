# vali3-docker

A self-contained Docker image of the FAI/CIVL IGC validation service, with the same JSON API as the original PHP server.

## Why

The official validation server at [vali.fai-civl.org](https://vali.fai-civl.org) is firewalled and only available to selected competitions. Anyone else who wants to validate IGC files has to run the old PHP stack on Windows to able to run the 32-bit Windows executables. This image packages those validators with Wine behind a small Rust HTTP server, so validating a flight is one `docker run` (or one HTTP request) away.

## Quick start

The validators are 32-bit Windows binaries, so the image must run as `linux/amd64`, even on Apple Silicon.

```sh
docker build --platform linux/amd64 -t vali3-docker .
docker run --rm --platform linux/amd64 -p 8080:8080 vali3-docker
```

Or with Compose:

```sh
docker compose up --build
```

Validate a file over HTTP:

```sh
curl -F 'igcfile=@vali3/downloads/sample-xmx.igc' \
     -F 'out=vali' \
     http://127.0.0.1:8080/api/vali/json
```

Check that the server is up:

```sh
curl -f http://127.0.0.1:8080/ready
```

You can also validate a single file without starting the server:

```sh
docker run --rm --platform linux/amd64 \
  -v "$PWD/vali3/downloads:/data:ro" \
  vali3-docker /data/sample-xmx.igc
```

Plain `.igc` files are accepted, as well as `.zip` (first entry) and `.gz` archives.

## Demo server

A demo instance runs at vali.vansaase.dev with the same API as the PHP server. It is intentionally resource constrained and strictly rate limited, so use it to try things out, not for bulk or production validation. Run your own container for that. The demo instance is provided as-is for testing, with no availability or security guarantees.

## API

`POST /api/vali/json` (also `/api/vali` and `/api.php` for compatibility with the PHP server) accepts `multipart/form-data`:

| Field     | Description                                                        |
| --------- | ------------------------------------------------------------------ |
| `igcfile` | The IGC, zip or gzip file to validate (required).                  |
| `out`     | `vali` returns the full validator output, `ncfail` only on no-C.   |
| `ref`     | Optional reference echoed back in the response.                    |
| `md5`     | Optional MD5 of the upload; the request fails if it doesn't match. |

The response mirrors the PHP server: `result`, `status`, `msg`, `igc`, `ref`, `server` and, when requested, `output`.

## Configuration

The server is configured with environment variables:

| Variable          | Default                     | Description                                               |
| ----------------- | --------------------------- | --------------------------------------------------------- |
| `BIND`            | `0.0.0.0:8080`              | Address and port the HTTP server listens on.              |
| `MAX_SIZE`        | `3145728` (3 MB)            | Maximum IGC size in bytes (also applies after unpacking). |
| `MAX_CONCURRENCY` | `8`                         | Maximum validations running at the same time.             |
| `TIMEOUT`         | `10`                        | Per-validation timeout in seconds.                        |
| `VALI_CONFIG`     | `vali3/vali-igc.conf`       | Validator module configuration file.                      |
| `VALI_MODULES`    | `vali3/cgi-bin`             | Directory containing the validator executables.           |
| `WINE`            | `wine`                      | Wine binary used to run the validators.                   |
| `IVSHELL`         | `vali3/cgi-bin/ivshell.exe` | The ivshell driver executable.                            |
| `RUST_LOG`        | `info`                      | Log level and filters (`tracing` env-filter syntax).      |

The image also uses Wine's `WINEPREFIX` (`/home/vali/.wine`) and keeps a `wineserver` running for the container's lifetime. Compose persists the prefix in a named volume; keep that volume if you build with `INIT_WINE=false` on Docker versions that block Wine's 32-bit socket syscalls.

## Security

**This service runs legacy, untrusted-input code (the validation tools) on untrusted input (the IGC files).**

The validators are closed-source 32-bit Windows executables running under Wine. Wine is a compatibility layer, not a sandbox. The docker container adds some guardrails: it runs as an unprivileged user, each validation gets a disposable temporary working directory, validations time out and are killed as a process group, and upload size and concurrency are capped.

There is no isolation beyond the container. A validator can read and write whatever the container user can, can use the network, and can consume CPU, memory and disk within the limits Docker gives it. If you expose the API to the internet, put appropriate risk mitigations in place.

## Development

`vali3/` is a git submodule with the original PHP sources and sample flights. The [compare workflow](.github/workflows/compare-with-php.yml) runs this image and the PHP server side by side against every sample in `vali3/downloads/` and diffs the JSON responses.
