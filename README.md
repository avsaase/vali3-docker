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

Validate a file over HTTP:

```sh
curl -F 'igcfile=@vali3/downloads/sample-xmx.igc' \
  -F 'out=vali' \
  http://127.0.0.1:8080/api/vali/json
```
