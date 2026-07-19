# The `apctl` CLI

`apctl` is a client of the panel's REST API, not a second way into the database. Config
generation, `angie -t` validation, the atomic swap with rollback and the audit trail all
stay in one place; the CLI only calls them.

It is its own crate, depending on nothing of the server's — no SQLite, no embedded
frontend, no systemd. That is what lets it run on a workstation the panel could never run
on, and ship as ~4 MB rather than the panel's ~15. On the panel host the `.deb` installs
it alongside the service; everywhere else you download one file.

Every command is also spelled `angie-panel ctl <command>` on the server, which links the
same crate — identical behaviour, so existing scripts can use either.

## Install

On the panel host, the `.deb` already brought it. Elsewhere, grab the archive for your
platform from the [latest release](https://github.com/maxname/angie-panel/releases/latest)
and put the binary on your `PATH`:

| Platform | Archive |
|---|---|
| Linux x86_64 | `apctl-<version>-x86_64-unknown-linux-musl.tar.gz` |
| Linux arm64 | `apctl-<version>-aarch64-unknown-linux-musl.tar.gz` |
| macOS Apple silicon | `apctl-<version>-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `apctl-<version>-x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `apctl-<version>-x86_64-pc-windows-msvc.zip` |
| Windows arm64 | `apctl-<version>-aarch64-pc-windows-msvc.zip` |

The Linux builds are static musl: one binary per architecture, no distro or glibc
requirements. Checksums and (when a signing key is configured) GPG signatures ship with
every release.

## On the server: nothing to set up

```bash
sudo apctl status
sudo apctl host add app.example.com --to http://127.0.0.1:3000
sudo apctl apply
```

The panel writes a machine-local token to `/var/lib/angie-panel/cli-token` (mode 0600) on
startup, and `apctl` reads it. `sudo` is there because the data directory is private to
the service account — not because the CLI needs root for anything else.

To rotate that token, delete the file and restart the service.

## From your workstation

Create a token in the panel under **API tokens**, then:

```bash
export ANGIE_PANEL_TOKEN=ap_…
export ANGIE_PANEL_URL=https://panel.example.com
apctl status
```

In PowerShell:

```powershell
$env:ANGIE_PANEL_TOKEN = "ap_…"
$env:ANGIE_PANEL_URL   = "https://panel.example.com"
apctl status
```

Both have flag equivalents (`--token`, `--url`), and a config file is not needed at all
when you pass them.

A token carries the role of the account that created it: a viewer's token can read but
not change anything, and demoting the account demotes the token with it. Tokens cannot
create further tokens — a leaked secret cannot mint successors that outlive revoking it.

## Commands

| Command | What it does |
|---|---|
| `apctl status` | Panel, Angie and D-Bus health, plus whether config is pending |
| `apctl diff` | The unified diff an apply would write (also `apply --dry-run`) |
| `apctl apply` | Generate, validate and reload; exits non-zero and prints `angie -t` output on failure |
| `apctl host ls` | Proxy hosts with upstream and state |
| `apctl host add <domains…> --to <url>` | Add a host (`--cert <id>`, `--websockets`) |
| `apctl host enable\|disable\|rm <id>` | Toggle or delete a host |
| `apctl cert ls` | Certificates and their issuance state |
| `apctl export` | The whole configuration as JSON on stdout |
| `apctl import <file>` | Load a dump (`-` reads stdin) |
| `apctl completions <shell>` | Completion script for bash/zsh/fish/elvish/powershell |

Editing a host is not in the CLI: the host editor has thirteen sections, and mapping all
of them onto flags would be worse than the UI at the job. Use `export`, edit the JSON,
and `import` it.

`--json` on any command prints the raw API response instead of the summary, which is the
intended way to script against it.

Nothing takes effect in Angie until `apctl apply` runs — the CLI says so after every
change, exactly like the panel does.

## Configuration as code

`export` and `import` are the pair worth building on:

```bash
apctl export > angie-panel.json          # commit this
apctl import angie-panel.json && apctl apply
```

That gives you a reviewable configuration in git and a one-command restore onto a fresh
box, without either side of it being a bespoke format — it is the panel's own dump.

## Shell completion

```bash
# bash, system-wide
apctl completions bash | sudo tee /etc/bash_completion.d/apctl >/dev/null

# zsh, into a directory on $fpath
apctl completions zsh > ~/.zfunc/_apctl
```

## Exit codes

`0` on success, `1` on any failure — an unreachable panel, a rejected token, a validation
error, or an apply that was rolled back. The reason goes to stderr, so `apctl apply ||
notify` works as you would expect in a deploy script.
