# Security policy

## Reporting a vulnerability

Please report security issues **privately**, not as a public issue.

Use GitHub's private vulnerability reporting:
[**Report a vulnerability**](https://github.com/maxname/angie-panel/security/advisories/new).
It is visible only to the maintainer until a fix ships.

Include what you need to make the problem reproducible — version, config shape, and the
request or steps involved. There is no bounty; this is a personal project.

## Supported versions

Fixes go onto `main` and into the next tagged release. Older tags are not patched — there
is one maintainer and no LTS branch. Run the latest release.

## Threat model

Read [docs/security.md](docs/security.md) before exposing the panel to anyone. The short
version, because it decides how you should deploy this:

**Whoever controls Angie's configuration controls the host.** Angie's master process runs
as root, and directives like `error_log`, `root` and `proxy_pass unix:` act with that
privilege. The panel writes that configuration. So access to the panel should be treated
as root access to the machine — the unprivileged service account limits the damage from a
*bug* in the panel, not from someone *taking it over*.

Consequences worth stating plainly:

- Do not put the panel on a public address. Bind it to a LAN interface, or publish it
  behind the proxy it manages with an access list in front.
- Anyone who can log in can execute code as root on that box, via config alone.
- The panel has had **no external security review**.

## What is hardened

- The service runs as `angie-panel`, with no write access to `/etc` and no sudo. The unit
  sets `NoNewPrivileges`, `ProtectSystem=strict`, an empty `CapabilityBoundingSet`,
  `SystemCallFilter=@system-service`, and restricted address families.
- Everything that writes to `/etc/angie/http.d/` goes through a small root helper: a
  oneshot systemd unit with a fixed `ExecStart` the panel cannot pass arguments to,
  reachable over D-Bus through a polkit rule scoped to exactly those units.
- Generated config is linted, then validated with `angie -t` on a staged copy before it is
  written, and rolled back from a snapshot if the reload fails.
- Sessions are cookie-based with argon2id password hashing; DNS provider credentials are
  encrypted at rest (ChaCha20-Poly1305) with the key stored beside the database — which
  protects a database that leaves the machine without its key, and nothing more.
- No CORS headers are ever emitted: the API is strictly same-origin.
