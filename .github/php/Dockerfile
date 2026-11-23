# CI-only image for the PHP reference implementation.
#
# The PHP code in vali3/ is run against the same wine installation as the Rust
# server so that both implementations see identical validator behaviour.
ARG BASE_IMAGE=vali3-docker:latest
FROM ${BASE_IMAGE}

USER root

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        php-cli \
        php-mbstring \
    && rm -rf /var/lib/apt/lists/*

COPY .github/php/entrypoint.sh /usr/local/bin/php-entrypoint.sh
RUN chmod 0755 /usr/local/bin/php-entrypoint.sh

USER vali
