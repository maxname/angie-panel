# ACME end-to-end harness

Proves that the Angie config **angie-panel generates** actually issues and
serves a real certificate — against a real ACME server, with a real Angie.

The product itself never uses Docker; this harness is a **test-only** artifact
(the plan calls for a pebble + Angie ACME e2e). It runs locally and in CI.

## What it verifies

The panel's M2 certificate model (PLAN.md §5) emits, per certificate:

```nginx
acme_client web https://<acme-directory>;
server {                                   # "collector" — never serves traffic
    listen unix:/run/angie-panel/acme-web.sock;
    server_name test.example.com;          # authoritative SAN
    acme web;                              # drives issuance
}
```

and, once the certificate is issued (`ready`), a serving block that carries
**no** `acme` directive — only the variables:

```nginx
server {
    listen 443 ssl;
    server_name test.example.com;
    ssl_certificate     $acme_cert_web;
    ssl_certificate_key $acme_cert_key_web;
    ...
}
```

`run.sh` stands up this exact structure and checks:

1. **Phase 1** — real Angie 1.11 obtains a cert from pebble via the collector
   block (`/status/http/acme_clients/` reports `certificate: valid`), and the
   cert lands in `/var/lib/angie/acme/web/`.
2. **Phase 2** — after adding the serving block and reloading, Angie serves
   HTTPS with the issued cert (`HTTP 200`).

This closes the one part of M2 that cannot be checked off-device: that the
generated structure genuinely issues certificates.

## Topology

Three containers on an IPv4-only bridge (static IPs so the mock DNS can point
the test domain at Angie):

| service | IP | role |
|---|---|---|
| `angie` | 10.30.0.10 | the reverse proxy under test (real Angie 1.11.8, `--with-http_acme_module`) |
| `pebble` | 10.30.0.20 | Let's Encrypt's test ACME server (directory at `https://pebble:14000/dir`) |
| `challtestsrv` | 10.30.0.30 | mock DNS — every A query resolves to `angie`; AAAA disabled so pebble uses IPv4 |

pebble's validation authority connects to `test.example.com:80` (→ angie);
Angie's ACME module answers the http-01 challenge. Angie trusts pebble's test
CA (`certs/pebble.minica.pem`, baked into the image) so it can fetch the
HTTPS ACME directory.

## Run

```sh
./run.sh          # builds, runs, asserts, tears down
KEEP=1 ./run.sh   # leave the containers up for inspection
```

Requires Docker + `docker compose`. Takes ~30–60s.

## Notes / gotchas encountered

- The status API returns **pretty-printed** JSON (`"certificate": "valid"`,
  with a space) — match tolerantly.
- pebble prefers IPv6; `challtestsrv -defaultIPv6 ""` disables AAAA so
  validation uses the IPv4 A record.
- The e2e config sets `retry_after_error=5s` (vs the panel's 2h default) so
  the harness converges quickly when pebble isn't ready at Angie's first
  attempt.
- `certs/pebble.minica.pem` is pebble's fixed test CA, fetched from the
  pebble repo. It is a **test** CA — not a secret, never used in production.
