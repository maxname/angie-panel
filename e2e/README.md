# ACME end-to-end harness

Proves that the Angie config **angie-panel generates** actually issues and
serves a real certificate — against a real ACME server, with a real Angie.

The product itself never uses Docker; this harness is a **test-only** artifact
(the plan calls for a pebble + Angie ACME e2e). It runs locally and in CI.

## The configs are generator output, not copies of it

Everything Angie loads here that the panel would own is written **by the
generator**, not by hand:

| file | what it is |
|---|---|
| `angie/http.d/10-acme.conf` | `gen_acme` output, with the ACME directory pointed at pebble |
| `angie/generated/pre/20-host-1-test-example-com.conf` | the host file before issuance (HTTP-only) |
| `angie/generated/ready/20-host-1-test-example-com.conf` | the same file after the cert is `ready` |
| `angie/http.d/90-harness.conf` | **hand-written scaffolding** — an origin to proxy to, and the status API. Not panel output. |
| `angie/angie.conf` | the packaged base config's include layout |

Regenerate after an intentional generator change, then review the diff:

```sh
UPDATE_E2E_FIXTURE=1 cargo test -p angie-panel e2e_
```

`generator::tests::e2e_acme_fixture_is_generator_output` and
`e2e_host_fixtures_are_generator_output` fail if the committed files drift, so
the harness cannot quietly end up proving something about a config the panel
never emits — which is exactly what a hand-maintained copy does over time.

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
2. **Phase 2** — the host file is overwritten with its `ready` state and
   reloaded (the panel's own re-apply, not a new file), after which Angie
   serves HTTPS with the issued cert (`HTTP 200`) — and phase 2b checks the
   body came from the origin, proving the generated `proxy_pass` carried it
   rather than some other block answering.
3. **Phase 3** — the pre-issuance file has no `listen 443`, so the `ready`
   gate really does keep the panel from referencing a cert Angie has not
   issued yet.

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
  attempt. That, and the pebble directory URL, are the only two deltas from
  production output — both applied in the test that writes the fixture, so
  they stay visible instead of drifting into hand edits.
- `certs/pebble.minica.pem` is pebble's fixed test CA, fetched from the
  pebble repo. It is a **test** CA — not a secret, never used in production.
