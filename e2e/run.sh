#!/usr/bin/env bash
# ACME end-to-end test: prove that the config angie-panel generates (acme_client
# + unix-socket collector, serving block referencing $acme_cert_<name> with NO
# acme directive) issues AND serves a real certificate — against a real ACME
# server (pebble) with real Angie 1.11.
#
# The configs under angie/generated/ and angie/http.d/10-acme.conf are real
# generator output, written by `UPDATE_E2E_FIXTURE=1 cargo test e2e_` and held
# to it by generator::tests — otherwise this harness would slowly drift into
# proving something about a config the panel never emits. angie/http.d/90-
# harness.conf is the only hand-written piece, and it is scaffolding (origin +
# status API), not panel output.
#
# Usage: e2e/run.sh   (requires docker + docker compose)
set -euo pipefail
cd "$(dirname "$0")"

COMPOSE="docker compose"
STATUS_URL="http://127.0.0.1:8100/status/http/acme_clients/"

pass() { printf '\033[32m✓ %s\033[0m\n' "$1"; }
fail() { printf '\033[31m✗ %s\033[0m\n' "$1"; }

cleanup() {
    if [ "${KEEP:-0}" != "1" ]; then
        $COMPOSE down -v >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

echo "==> Building + starting the harness (angie + pebble + challtestsrv)"
$COMPOSE up -d --build

echo "==> Phase 1: waiting for Angie to issue the certificate (collector block)"
issued=""
for i in $(seq 1 40); do
    sleep 2
    body="$(curl -fsS "$STATUS_URL" 2>/dev/null || echo CONNFAIL)"
    cert="$(printf '%s' "$body" | grep -oE '"certificate": *"[a-z]*"' | head -1 || true)"
    echo "    [$((i * 2))s] ${cert:-${body:0:40}}"
    if printf '%s' "$body" | grep -qE '"certificate": *"valid"'; then
        issued=1
        break
    fi
done
if [ -z "$issued" ]; then
    fail "certificate was NOT issued within the timeout"
    echo "--- angie log ---"; $COMPOSE logs --tail=30 angie || true
    echo "--- pebble log ---"; $COMPOSE logs --tail=20 pebble || true
    exit 1
fi
pass "Phase 1: real Angie issued a cert from pebble (acme_client + collector)"

echo "==> Verifying the issued cert lives in Angie's ACME store"
if $COMPOSE exec -T angie sh -c '[ -s /var/lib/angie/acme/web/certificate.pem ]'; then
    pass "Phase 1b: /var/lib/angie/acme/web/certificate.pem present"
else
    fail "cert file missing"
    exit 1
fi

echo "==> Phase 2: re-apply the same host file in its ready state (as the panel does) + reload"
# Not a new file: the panel flips the cert to `ready` and rewrites the host file
# it already owns. Overwriting it here is that same step.
$COMPOSE exec -T angie sh -c \
    'cp /tmp/generated/ready/*.conf /etc/angie/extra.d/ && angie -t && angie -s reload' >/dev/null
sleep 2
out="$($COMPOSE exec -T angie sh -c 'curl -sSk -o /dev/null -w "%{http_code}" https://127.0.0.1/ -H "Host: test.example.com"')"
if [ "$out" = "200" ]; then
    pass "Phase 2: Angie serves HTTPS with the issued cert (HTTP 200)"
else
    fail "HTTPS serving returned HTTP $out (expected 200)"
    $COMPOSE logs --tail=30 angie || true
    exit 1
fi

# A 200 could come from anywhere; prove it came through the generated
# proxy_pass, from the origin behind it.
body="$($COMPOSE exec -T angie sh -c 'curl -sSk https://127.0.0.1/ -H "Host: test.example.com"')"
if printf '%s' "$body" | grep -q "e2e origin"; then
    pass "Phase 2b: the response came from the origin via the generated proxy_pass"
else
    fail "HTTPS body did not come from the origin (got: ${body:0:60})"
    exit 1
fi

echo "==> Phase 3: the pre-issuance state is HTTP-only (the \`ready\` gate)"
# The panel must never emit a 443 block for a cert Angie hasn't issued, or a
# reload would fail on a missing certificate. The generated pre-issuance file
# is what proves the gate holds.
if grep -q "listen 443" angie/generated/pre/*.conf; then
    fail "the pre-issuance host file has a 443 block — the ready gate is broken"
    exit 1
fi
pass "Phase 3: generated pre-issuance host is HTTP-only"

echo
pass "ACME e2e PASSED — issuance + HTTPS serving on the panel's config structure"
