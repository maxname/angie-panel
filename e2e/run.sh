#!/usr/bin/env bash
# ACME end-to-end test: prove that the config structure angie-panel generates
# (acme_client + unix-socket collector, serving block referencing
# $acme_cert_<name> with NO acme directive) issues AND serves a real
# certificate — against a real ACME server (pebble) with real Angie 1.11.
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

echo "==> Phase 2: add the HTTPS serving block (references \$acme_cert_web) + reload"
$COMPOSE exec -T angie sh -c \
    'cat /tmp/host-https.conf > /etc/angie/extra.d/30-host-https.conf && angie -t && angie -s reload' >/dev/null
sleep 2
out="$($COMPOSE exec -T angie sh -c 'curl -sSk -o /dev/null -w "%{http_code}" https://127.0.0.1/ -H "Host: test.example.com"')"
if [ "$out" = "200" ]; then
    pass "Phase 2: Angie serves HTTPS with the issued cert (HTTP 200)"
else
    fail "HTTPS serving returned HTTP $out (expected 200)"
    exit 1
fi

echo
pass "ACME e2e PASSED — issuance + HTTPS serving on the panel's config structure"
