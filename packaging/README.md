# angie-panel — packaging

Packaging artifacts for **angie-panel**, a web configurator for the Angie
reverse proxy, deployed as a native systemd service on Debian 12/13
(arm64 / amd64; primary target: Armbian SBCs such as the NanoPi R4S).

## Contents

| File | Installed to | Purpose |
|---|---|---|
| `systemd/angie-panel.service` | `/usr/lib/systemd/system/` | The panel itself. Runs as the unprivileged `angie-panel` user with full sandboxing. |
| `systemd/angie-panel-configtest.service` | `/usr/lib/systemd/system/` | Root oneshot helper: validates the live Angie config (`angie -t`), touches nothing. |
| `systemd/angie-panel-apply.service` | `/usr/lib/systemd/system/` | Root oneshot helper: full apply pipeline (lint, validate, snapshot, atomic sync, reload, rollback on failure). |
| `systemd/angie-panel-enable-streams.service` | `/usr/lib/systemd/system/` | Root oneshot helper: activates the `stream {}` context in angie.conf (needed before TCP/UDP streams apply). |
| `polkit/10-angie-panel.rules` | `/usr/share/polkit-1/rules.d/` | Authorizes the `angie-panel` user to start exactly the three helper units — nothing else. |
| `angie-panel.toml` | `/etc/angie-panel.toml` (dpkg conffile) | Default panel configuration. Root-owned; privilege-relevant switches live here, not in the panel DB. |
| `debian/postinst`, `debian/prerm`, `debian/postrm` | maintainer scripts | For `cargo-deb` (with the `systemd-units` feature, which fills in the `#DEBHELPER#` token). Create/remove the service account and `/var/lib/angie-panel`. |
| `install.sh` | run by the user | End-user installer: preflight checks, Angie apt repo, both packages, safe bind-address selection, setup token output. Messages are in Russian (target audience). |

Package dependencies (set manually in `Cargo.toml` / cargo-deb metadata —
auto-detection is empty for a static musl binary):
`angie (>= 1.11), dbus, polkitd`.

## Security model

- **The panel is unprivileged.** `angie-panel.service` runs as the
  `angie-panel` system user with `ProtectSystem=strict`,
  `NoNewPrivileges=yes`, an empty capability bounding set, and syscall
  filtering. Its only writable path is `/var/lib/angie-panel` (mode `0700`,
  except `public/` which is `0755` and served by Angie workers). It never
  writes to `/etc`.
- **Two root oneshot units are the trust boundary.** Writing to
  `/etc/angie/http.d` is root-equivalent (the config is executed by Angie's
  root master), so that step is isolated in a small auditable helper —
  a subcommand of the same binary, runnable only via two units with **fixed
  `ExecStart` lines and no arguments**. The panel controls *whether* they
  run, never *what* they run.
- **polkit scopes the bridge.** The panel starts the helpers via D-Bus
  (`org.freedesktop.systemd1` `StartUnit`). The rule in
  `10-angie-panel.rules` allows `subject.user == "angie-panel"` to use
  action `org.freedesktop.systemd1.manage-units` only with `verb == "start"`
  and `unit` equal to one of the two helper unit names; everything else
  returns `NOT_HANDLED` and falls through to the default deny. Requires
  polkit >= 0.106 (JS rules), hence Debian 12+ only.
- The helper itself reloads Angie (`systemctl reload angie`) — the panel
  needs no rights on `angie.service`.
- First-run auth uses a one-time setup token in
  `/var/lib/angie-panel/setup-token` (`0600`, owner `angie-panel`).
  `angie-panel reset-password` generates a new one without touching data.

## Manual installation (without install.sh)

1. Add the official Angie repository (Debian 12 shown; use `13`/`trixie` on
   trixie):

   ```sh
   apt-get update && apt-get install -y ca-certificates curl
   curl -o /etc/apt/trusted.gpg.d/angie-signing.gpg https://angie.software/keys/angie-signing.gpg
   echo "deb https://download.angie.software/angie/debian/12 bookworm main" \
       > /etc/apt/sources.list.d/angie.list
   apt-get update && apt-get install -y angie
   systemctl enable --now angie
   ```

2. Install the panel package (resolves `dbus`/`polkitd` dependencies):

   ```sh
   apt-get install -y ./angie-panel_<version>_arm64.deb
   ```

3. Recommended: remove the packaged Angie default site (it is a dpkg
   conffile — the removal survives Angie upgrades; the panel provides its
   own default site):

   ```sh
   rm /etc/angie/http.d/default.conf && systemctl reload angie
   ```

4. Edit `/etc/angie-panel.toml`: set `bind_addr` to a **LAN** address
   (never a WAN-facing one — publish the panel through Angie instead).

5. Start and fetch the setup token:

   ```sh
   systemctl enable --now angie-panel
   cat /var/lib/angie-panel/setup-token
   # open http://<bind_addr>:8080/setup
   ```

### Fully by hand (no .deb, e.g. for development)

```sh
install -m 0755 target/.../angie-panel /usr/bin/angie-panel
install -m 0644 packaging/systemd/*.service /usr/lib/systemd/system/
install -m 0644 packaging/polkit/10-angie-panel.rules /usr/share/polkit-1/rules.d/
install -m 0644 packaging/angie-panel.toml /etc/angie-panel.toml
adduser --system --group --home /var/lib/angie-panel --no-create-home \
        --shell /usr/sbin/nologin angie-panel
install -d -o angie-panel -g angie-panel -m 0700 /var/lib/angie-panel
install -d -o angie-panel -g angie-panel -m 0755 /var/lib/angie-panel/public
systemctl daemon-reload
systemctl enable --now angie-panel
```

polkit picks up new rules files automatically; restart `polkit.service` if
in doubt.

## Fallback without polkit (sudoers)

On systems where polkit is unavailable, the same "only these two fixed
commands" property can be approximated with sudo:

```
angie-panel ALL=(root) NOPASSWD: /usr/bin/systemctl start angie-panel-configtest.service, /usr/bin/systemctl start angie-panel-apply.service, /usr/bin/systemctl start angie-panel-enable-streams.service
```

(the polkit rule authorizes three units — configtest, apply and
enable-streams — so the sudo fallback must cover all three, or the "Enable
streams" action breaks.)

(put it in `/etc/sudoers.d/angie-panel`, mode `0440`, validate with
`visudo -c`).

**Caveat:** `NoNewPrivileges=yes` in `angie-panel.service` makes sudo
impossible and must be disabled for this fallback. Note that many other
sandbox options in the unit (`SystemCallFilter=`, `SystemCallArchitectures=`,
`RestrictAddressFamilies=`, `RestrictNamespaces=`, `PrivateDevices=`,
`MemoryDenyWriteExecute=`, `RestrictRealtime=`, `RestrictSUIDSGID=`,
`LockPersonality=`, `ProtectKernelModules=`, `ProtectKernelLogs=`,
`ProtectKernelTunables=`, `ProtectControlGroups=`, `ProtectClock=`,
`ProtectHostname=`) *implicitly enable* NoNewPrivileges for non-root
services — they must be reset in the override too (see `systemd.exec(5)`).
Example drop-in
(`/etc/systemd/system/angie-panel.service.d/sudo-fallback.conf`):

```ini
[Service]
NoNewPrivileges=no
SystemCallFilter=
SystemCallArchitectures=
RestrictAddressFamilies=
RestrictNamespaces=no
PrivateDevices=no
MemoryDenyWriteExecute=no
RestrictRealtime=no
RestrictSUIDSGID=no
LockPersonality=no
ProtectKernelModules=no
ProtectKernelLogs=no
ProtectKernelTunables=no
ProtectControlGroups=no
ProtectClock=no
ProtectHostname=no
```

This substantially weakens the sandbox — the polkit path is strongly
preferred and is the only supported configuration on Debian 12/13.
