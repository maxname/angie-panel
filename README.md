# Angie Panel

> Рабочее название проекта. [English version below.](#english)

![status: M0 skeleton](https://img.shields.io/badge/status-M0_skeleton-orange)
![license: TBD](https://img.shields.io/badge/license-TBD-lightgrey)

**Angie Panel** — веб-конфигуратор reverse-proxy на базе [Angie](https://angie.software/): аналог nginx-proxy-manager, но нативный systemd-сервис без Docker и с сертификатами через встроенный ACME-модуль Angie.

**Статус:** ранняя разработка, этап **M0 («каркас»)**. Не используйте в продакшене.

## Чем отличается

- **Встроенный ACME вместо certbot** — Angie сам выпускает и продлевает сертификаты (http-01, tls-alpn-01, dns-01/wildcard); никаких certbot/pip/cron.
- **Честный apply-пайплайн** — diff перед применением, валидация staging-конфига через `angie -t`, атомарная запись и автоматический откат по снапшоту при любой ошибке.
- **Панель без root** — конфиги генерирует непривилегированный пользователь; в `/etc` пишет только маленький аудируемый root-хелпер (oneshot systemd-юнит, доступ строго через polkit).
- **arm64 SBC — first-class target** — разрабатывается под NanoPi R4S (Armbian, Debian 12/13); один статический musl-бинарник, ~10–20 МБ RAM.

## Архитектура (кратко)

Бэкенд на Rust (axum) со встроенным React UI работает от пользователя `angie-panel` и хранит всё в SQLite — конфиги Angie являются детерминированной проекцией БД. Изменения применяются через тонкий root-хелпер (тот же бинарник, отдельные oneshot-юниты за polkit): линтер → `angie -t` на staging-конфиге → снапшот → атомарная синхронизация в `/etc/angie/http.d/` → graceful reload, при сбое — откат. Сертификаты выпускает и продлевает сам Angie через встроенный ACME-модуль.

Подробности: [PLAN.md](PLAN.md) и [docs/research/](docs/research/).

## Разработка

Бэкенд:

```bash
# однократно, если фронтенд ещё не собран (rust-embed требует каталог при компиляции):
mkdir -p frontend/dist && touch frontend/dist/index.html

cd backend
cargo run -- serve --config ../dev/angie-panel.toml
```

Фронтенд (dev-сервер Vite):

```bash
cd frontend
pnpm install
pnpm dev
```

Панель доступна на <http://127.0.0.1:5173>, запросы к `/api` проксируются на бэкенд (порт 8080).

## Установка на устройство

Появится после первого релиза (M0). Планируемый вид — one-liner на основе [packaging/install.sh](packaging/install.sh):

```bash
curl -fsSL https://github.com/OWNER/REPO/releases/latest/download/install.sh -o install.sh
less install.sh   # сначала прочитайте скрипт
sudo bash install.sh
```

## Дорожная карта

| Этап | Кратко |
|---|---|
| **M0** | Каркас и фундамент привилегий: repo/CI/cross-сборка, auth + setup-токен, .deb + systemd-юниты + polkit, root-хелпер end-to-end |
| **M1** | Хосты и apply: CRUD proxy-хостов, генератор + линтер, staging-валидация, снапшоты и rollback, детект дрейфа |
| **M2** | Сертификаты: встроенный ACME (http-01 / tls-alpn-01 / dns-01 wildcard), state machine первого выпуска, статусы продления |
| **M3** | Дашборд: /status-поллинг, сверка фактического конфига, история apply, алерты |
| **M4** | Релиз v1.0: экспорт/импорт, документация RU/EN, подпись артефактов, e2e на устройстве |

Полная дорожная карта — в [PLAN.md](PLAN.md), §10.

## Лицензия

TBD.

---

## English

**Angie Panel** (working title) is a web-based reverse-proxy configurator built on [Angie](https://en.angie.software/) — think nginx-proxy-manager, but a native systemd service (no Docker) with certificates handled by Angie's built-in ACME module.

**Status:** early development, milestone **M0 (skeleton)**. Not production-ready.

Key differentiators:

- **Built-in ACME instead of certbot** — Angie itself issues and renews certificates (http-01, tls-alpn-01, dns-01/wildcard).
- **Honest apply pipeline** — diff preview, staged `angie -t` validation, atomic sync, automatic snapshot rollback on any failure.
- **The panel never runs as root** — config generation is unprivileged; only a small auditable root helper (oneshot systemd unit behind polkit) writes to `/etc`.
- **arm64 SBCs are a first-class target** — developed for the NanoPi R4S (Armbian, Debian 12/13); a single static musl binary.

Architecture in short: a Rust (axum) backend with an embedded React UI stores everything in SQLite; Angie config files are a deterministic projection of the database, applied through the root helper (lint → staged `angie -t` → snapshot → atomic sync into `/etc/angie/http.d/` → graceful reload, rollback on failure). Details: [PLAN.md](PLAN.md) (Russian) and [docs/research/](docs/research/).

Dev quickstart:

```bash
# backend (once: mkdir -p frontend/dist && touch frontend/dist/index.html for rust-embed)
cd backend && cargo run -- serve --config ../dev/angie-panel.toml

# frontend
cd frontend && pnpm install && pnpm dev
```

Panel at <http://127.0.0.1:5173>; `/api` is proxied to the backend on port 8080.

Install on a device: a one-liner based on [packaging/install.sh](packaging/install.sh), coming with the first release.

Roadmap: **M0** skeleton & privilege model → **M1** proxy hosts & apply pipeline → **M2** certificates via built-in ACME → **M3** live dashboard & drift detection → **M4** v1.0 release (export/import, docs, signed artifacts). See [PLAN.md](PLAN.md) §10.

License: TBD.
