# Real Angie (1.11.x, http_acme module built in) that trusts pebble's test CA
# so acme_client can fetch pebble's HTTPS ACME directory.
FROM docker.angie.software/angie:latest
USER root
COPY certs/pebble.minica.pem /usr/local/share/ca-certificates/pebble-minica.crt
RUN apk add --no-cache ca-certificates \
    && update-ca-certificates \
    && mkdir -p /run/angie-panel /var/log/angie
