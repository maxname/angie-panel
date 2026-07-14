# Real Angie (http_acme module built in) that trusts pebble's test CA so
# acme_client can fetch pebble's HTTPS ACME directory. Pinned to a concrete
# tag for reproducible CI — bump deliberately (keep e2e/README in sync).
FROM docker.angie.software/angie:1.11.8
USER root
COPY certs/pebble.minica.pem /usr/local/share/ca-certificates/pebble-minica.crt
RUN apk add --no-cache ca-certificates \
    && update-ca-certificates \
    && mkdir -p /run/angie-panel /var/log/angie
