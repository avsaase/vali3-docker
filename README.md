Build the image:

```sh
docker build --platform linux/amd64 -t vali3-docker .
```

Validate one file with a single-file bind mount:

```sh
docker run --rm --platform linux/amd64 \
  -v "$PWD/vali3/downloads/sample-xmx.igc:/data/sample.igc:ro" \
  vali3-docker /data/sample.igc
```

Validate one file from a mounted directory:

```sh
docker run --rm --platform linux/amd64 \
  -v "$PWD/vali3/downloads:/data:ro" \
  vali3-docker /data/sample-xmx.igc
```

Start the server:

```sh
docker run --rm --platform linux/amd64 -p 8080:8080 vali3-docker
```

Or with Docker Compose:

```sh
docker compose up --build
```

The image pre-initializes the Wine prefix during the build, so container
startup is fast. If your Docker version blocks the 32-bit socket syscalls Wine
needs (for example 29.4.2), build with `--build-arg INIT_WINE=false` and keep
`/home/vali/.wine` in a named volume; the entrypoint will initialize it on the
first start.

Validate a file over HTTP:

```sh
curl -F 'igcfile=@vali3/downloads/sample-xmx.igc' \
  -F 'out=vali' \
  http://127.0.0.1:8080/api/vali/json
```

Check that the server is ready:

```sh
curl -f http://127.0.0.1:8080/ready
```

## Comparing with the PHP implementation

The [Compare Docker server with PHP server](.github/workflows/compare-with-php.yml)
workflow runs the Docker image and the PHP implementation from `vali3/` side by
side, posts every `vali3/downloads/*.igc` sample to both, and compares the JSON
responses.

Both servers run in the same wine-based image so the validators behave the
same. The PHP code targets Windows, so `.github/scripts/prepare_php_server.py`
creates a CI-only copy that uses POSIX paths, resolves module names
case-insensitively and hands the validator executables to wine. The workflow
uses Docker BuildKit with the GitHub Actions cache, so Rust and PHP image layers
are reused between runs.
