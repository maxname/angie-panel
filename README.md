# Angie Panel

> Рабочее название проекта. [English version below.](#english)

![status: M4 feature-complete](https://img.shields.io/badge/status-M4_feature--complete-brightgreen)
![license: TBD](https://img.shields.io/badge/license-TBD-lightgrey)

**Angie Panel** — веб-конфигуратор reverse-proxy на базе [Angie](https://angie.software/): аналог nginx-proxy-manager, но нативный systemd-сервис без Docker и с сертификатами через встроенный ACME-модуль Angie.

**Статус:** функционально готов (этапы M0–M4). Реальный выпуск сертификата проверяется на каждый push в CI (Angie + pebble). Осталась проверка на самом устройстве (systemd-reload на живом железе).

**Документация:** [установка](docs/installation.md) · [сертификаты/ACME](docs/certificates.md) · [безопасность](docs/security.md) · [диагностика](docs/troubleshooting.md).

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

Подробности и первый вход — в [docs/installation.md](docs/installation.md). Кратко:

```bash
curl -fsSL https://github.com/maxname/angie-panel/releases/latest/download/install.sh -o install.sh
less install.sh   # сначала прочитайте скрипт
sudo bash install.sh
```

Дефолтного пароля нет: при первом запуске возьмите одноразовый setup-токен
(`sudo cat /var/lib/angie-panel/setup-token`) и создайте администратора на `/setup`.

## Дорожная карта

| Этап | Кратко | Статус |
|---|---|---|
| **M0** | Каркас и фундамент привилегий: repo/CI/cross-сборка, auth + setup-токен, .deb + systemd-юниты + polkit, root-хелпер | ✅ |
| **M1** | Хосты и apply: CRUD proxy-хостов, генератор + линтер, staging-валидация, снапшоты и rollback, детект дрейфа | ✅ |
| **M2** | Сертификаты: встроенный ACME (http-01 / tls-alpn-01 / dns-01 wildcard), state machine первого выпуска; проверено на Angie+pebble | ✅ |
| **M3** | Дашборд: /status-поллинг, метрики по хостам, статусы сертификатов, детект дрейфа, авто-включение HTTPS после выпуска | ✅ |
| **M4** | Экспорт/импорт конфигурации (JSON), документация RU/EN, GPG-подпись артефактов | ✅ |

Осталось: e2e-прогон на реальном NanoPi R4S (systemd-reload и выпуск на живом железе).
Полная дорожная карта — в [PLAN.md](PLAN.md), §10.

## Лицензия

TBD.

---

## English

**Angie Panel** (working title) is a web-based reverse-proxy configurator built on [Angie](https://en.angie.software/) — think nginx-proxy-manager, but a native systemd service (no Docker) with certificates handled by Angie's built-in ACME module.

**Status:** feature-complete (milestones M0–M4). Real certificate issuance is exercised on
every CI push (Angie + pebble); on-device verification (systemd reload on real hardware) is
what remains. Docs: [installation](docs/installation.md) ·
[certificates/ACME](docs/certificates.md) · [security](docs/security.md) ·
[troubleshooting](docs/troubleshooting.md).

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

Install on a device: see [docs/installation.md](docs/installation.md). There is no default
password — read the one-time setup token (`sudo cat /var/lib/angie-panel/setup-token`) and
create the admin at `/setup`.

Roadmap: **M0** skeleton & privilege model → **M1** proxy hosts & apply pipeline → **M2**
certificates via built-in ACME → **M3** live dashboard & auto-HTTPS → **M4** export/import,
docs, signed artifacts — all ✅. On-device e2e on the NanoPi R4S is what remains.
See [PLAN.md](PLAN.md) §10.

Roadmap: **M0** skeleton & privilege model → **M1** proxy hosts & apply pipeline → **M2** certificates via built-in ACME → **M3** live dashboard & drift detection → **M4** v1.0 release (export/import, docs, signed artifacts). See [PLAN.md](PLAN.md) §10.

License: TBD.
