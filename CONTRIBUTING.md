# Contributing

Thanks for looking. This is a small project with one maintainer, so a short issue
describing what you hit is genuinely useful — even without a patch.

## Reporting

- **Bug:** what you did, what happened, what you expected. Include the panel version
  (`angie-panel --version`), the Angie version, and the relevant slice of
  `journalctl -u angie-panel`. If it involves generated config, the file from
  `/etc/angie/http.d/` says more than a description of it.
- **Security issue:** do not open an issue — see [SECURITY.md](SECURITY.md).
- **Feature:** describe the problem before the solution. What are you trying to do that
  the panel makes hard?

## Getting it running

```bash
# backend — rust-embed needs the frontend directory to exist at compile time
mkdir -p frontend/dist && touch frontend/dist/index.html
cd backend && cargo run -- serve --config ../dev/angie-panel.toml

# frontend, in another shell
cd frontend && pnpm install && pnpm dev
```

The UI is at <http://127.0.0.1:5173>; `/api` proxies to the backend on 8080. You do not
need Angie installed to work on most things — the panel runs without it and simply reports
it as down.

## Checks

CI runs all of these on every push, so run them before opening a PR:

```bash
cd backend
cargo fmt --check
cargo clippy --all-targets     # warnings are errors in CI
cargo test

cd ../frontend
pnpm lint
pnpm typecheck
pnpm test
pnpm build
```

Two things that will bite you:

- **`npx tsc --noEmit` checks nothing here.** The tsconfig is solution-style (`"files": []`
  plus references), so it exits 0 on a broken tree. Use `pnpm typecheck` (`tsc -b`) or
  `pnpm build`.
- **Vite does not typecheck.** A green `pnpm dev` proves nothing about types.

The end-to-end test needs Docker and runs a real Angie against
[pebble](https://github.com/letsencrypt/pebble):

```bash
cd e2e && ./run.sh
```

## Changing the generated config

Config generation is covered by golden files in `backend/tests/golden/`. If your change
alters output, regenerate them and read the diff before committing it:

```bash
cd backend && UPDATE_GOLDEN=1 cargo test
```

Goldens compare strings, so they prove the output did not change unintentionally — not
that Angie accepts it. If you touch the shape of a server block, validate it with a real
`angie -t`, not just a green test. There is precedent: a `proxy_pass` change once passed
every golden and was rejected by Angie at load.

## Database changes

Migrations are files in `backend/migrations/`, applied in order and never edited after
release. Add a new one; do not modify an existing file. Take the next free number — check
what is already there, since an unmerged branch may have claimed one.

If a migration touches existing rows, say in a comment what it does to data that is
already out there, and test it against a copy of a real database, not just an empty one.

## Commits and pull requests

- Keep a PR to one thing. Several unrelated fixes are easier to review as several PRs.
- Commit messages: a short imperative subject, then a body explaining *why* — what was
  broken, what you tried, what the trade-off is. The history here is written to be read
  later; "fix bug" costs the next person an archaeology session.
- New behaviour comes with a test. A test that passes before your fix is not a test of
  your fix — check that it fails without it.
- UI strings go in both `frontend/src/i18n/en.ts` and `ru.ts`. A parity test fails if a key
  exists in one and not the other, or if code references a key that does not exist.

## Layout

```
frontend/    React 19 + Vite + Tailwind + shadcn/ui
backend/     Rust: axum, sqlx/SQLite, config generator, ACME hook, root helper
packaging/   .deb metadata, systemd units, polkit rules, install.sh
e2e/         real Angie + pebble
docs/        installation, certificates, security, troubleshooting
```

Design notes and the original plan are in [PLAN.md](PLAN.md) and
[docs/research/](docs/research/) — both in Russian.
