# Angie Panel

> 🇬🇧 [Read in English](README.md) · 🌐 [сайт проекта](https://maxname.github.io/angie-panel/)

[![CI](https://github.com/maxname/angie-panel/actions/workflows/ci.yml/badge.svg)](https://github.com/maxname/angie-panel/actions/workflows/ci.yml)
![license: MIT](https://img.shields.io/badge/license-MIT-blue)
![arm64 · amd64](https://img.shields.io/badge/arch-arm64%20%C2%B7%20amd64-informational)

Веб-интерфейс для [Angie](https://angie.software/) в роли reverse-proxy — то же, что
nginx-proxy-manager, только обычный systemd-сервис вместо докер-стека, а сертификаты
выпускает встроенный в Angie ACME-модуль, а не certbot.

Сделано для маленьких машин, которые работают круглосуточно: один статический бинарник,
никаких зависимостей в рантайме, 10–20 МБ памяти.

![Прокси-хосты с полосами аптайма](docs/screenshots/hosts.png)

## Чем отличается от nginx-proxy-manager

- **Сертификаты выпускает Angie, а не certbot.** http-01, tls-alpn-01 и dns-01 (включая
  wildcard) обслуживает встроенный ACME-модуль. Ни certbot, ни pip, ни крона на продление —
  и не нужно держать живым контейнер только ради сертификата.
- **Применение конфига не врёт.** Сначала показывается diff, потом staging-копия
  проверяется через `angie -t`, пишется атомарно и откатывается из снапшота, если reload
  не удался. Ручные правки файлов на диске (дрейф) панель замечает и показывает.
- **Панель не работает под root.** Конфиги генерирует непривилегированный пользователь;
  в `/etc` пишет только маленький аудируемый хелпер — тот же бинарник, запускаемый
  oneshot-юнитом systemd строго через polkit.
- **Никакого Docker.** Один `.deb`, один systemd-сервис, один файл SQLite.
- **Мониторинг доступности встроен.** TCP- и HTTP(S)-проверки на каждый хост с историей,
  полоса аптайма прямо в списке хостов.
- **Полноценный CLI, а не только веб-морда.** `apctl status`, `apctl apply` и пара
  `export`/`import` для конфигурации в git. Он ходит в тот же API, что и браузер, поэтому
  скриптовые изменения проходят ту же валидацию, откат и аудит. Поставляется отдельным
  бинарником ~4 МБ под Linux, macOS и Windows, arm64 и x86_64. См. [docs/cli.md](docs/cli.md).

## Скриншоты

| | |
|---|---|
| **Дашборд** — метрики Angie, состояние сертификатов, трафик по хостам | **Сертификаты** — выпускает Angie: http-01 / dns-01 / wildcard |
| ![Дашборд](docs/screenshots/dashboard.png) | ![Сертификаты](docs/screenshots/certificates.png) |
| **Редактор хоста** — тринадцать разделов, от SSL до лимитов | **Настройки** — значения по умолчанию для всей установки |
| ![Редактор хоста](docs/screenshots/editor.png) | ![Настройки](docs/screenshots/settings.png) |

## Установка

Debian/Ubuntu на `amd64` или `arm64`, Angie уже установлен (полная инструкция с нуля —
[docs/deploy-nanopi.md](docs/deploy-nanopi.md)):

```bash
curl -fsSL https://github.com/maxname/angie-panel/releases/latest/download/install.sh -o install.sh
less install.sh          # сначала прочитайте скрипт
sudo bash install.sh
```

Пароля по умолчанию нет. Возьмите одноразовый setup-токен и создайте первого администратора:

```bash
sudo cat /var/lib/angie-panel/setup-token
```

Дальше откройте `http://<хост>:8080/setup`. Подробности и обновление —
[docs/installation.md](docs/installation.md).

Вместе с пакетом ставится `apctl` — CLI к панели; на сервере токен ему не нужен. Чтобы
управлять панелью со своей машины, поставьте его через Homebrew
(`brew install maxname/tap/apctl`, macOS и Linux) или скачайте отдельный бинарник под свою
платформу. Про оба способа — в [руководстве по CLI](docs/cli.md).

В релизах лежат `.deb` для обеих архитектур, отдельные бинарники `apctl` под шесть
платформ, `SHA256SUMS.txt` и отсоединённые GPG-подписи. Сверять нужно с этим отпечатком, а
не с ключом из самого релиза — тот сам по себе ничего не доказывает:

```
E81C 9989 402A 5C15 B0DD  21B9 2177 3F03 FDFC 43ED
```

Как проверять: [docs/installation.md](docs/installation.md#проверка-подписи-артефактов).

## Что умеет

**Хосты.** Прокси-хосты, хосты перенаправления, 404-хосты, TCP/UDP-потоки и SNI-роутеры.
На каждый хост: websockets, HTTP/2 и HTTP/3, HSTS, свои локации, апстримы с балансировкой,
лимиты запросов, свои заголовки, gzip, страницы ошибок, режим обслуживания, mTLS,
forward-auth и «аварийный люк» для произвольного сниппета.

**Сертификаты.** Выпускает и продлевает Angie. DNS-01 ходит в API провайдеров через
вендоренный `acme.sh` (Cloudflare, reg.ru, Route 53 и другие); хук ждёт, пока TXT-запись
разъедется по **всем** авторитетным NS, и только потом отдаёт УЦ команду проверять.

**Доступность.** TCP- и HTTP(S)-проверки на хост, включаются по желанию, у каждой свой
интервал. HTTP-проверка идёт по петле с доменом в SNI и проверяет сертификат — то есть
отвечает на вопрос «отдаёт ли сайт *мой* Angie». Сказать, что до него достучится интернет,
она не может — как и всё, что запущено на этой же машине.

**Безопасность.** Списки доступа (basic-аутентификация + правила по IP), блок-лист IP,
geo-политика по странам, журнал аудита, пользователи с ролями.

**Эксплуатация.** Экспорт/импорт конфигурации в JSON, история применений, детект дрейфа и
дашборд на данных из status API самого Angie.

## Архитектура

Бэкенд на Rust ([axum](https://github.com/tokio-rs/axum)) с React-интерфейсом, вшитым в
бинарник. Всё состояние — в SQLite; файлы в `/etc/angie/http.d/` являются детерминированной
проекцией базы и руками не правятся.

Применение идёт так: линтер → `angie -t` на staging-копии → снапшот → атомарная
синхронизация → graceful reload, и автоматический откат, если что-то сорвалось.
Привилегированная часть — несколько oneshot-юнитов systemd, которые панель может запустить
через polkit, и больше ничего.

```
frontend/    React 19 + Vite + Tailwind + shadcn/ui
backend/     Rust: axum, sqlx/SQLite, генератор конфигов, ACME-хук, root-хелпер
packaging/   метаданные .deb, systemd-юниты, правила polkit, install.sh
e2e/         настоящий Angie + pebble, сквозной прогон выпуска сертификата
docs/        установка, сертификаты, CLI, безопасность, диагностика
```

Подробнее: [PLAN.md](PLAN.md) и [docs/research/](docs/research/).

## Разработка

```bash
# бэкенд — rust-embed требует, чтобы каталог фронтенда существовал на этапе компиляции
mkdir -p frontend/dist && touch frontend/dist/index.html
cd backend && cargo run -- serve --config ../dev/angie-panel.toml

# фронтенд
cd frontend && pnpm install && pnpm dev
```

Интерфейс на <http://127.0.0.1:5173>, запросы `/api` проксируются на бэкенд (порт 8080).

Проверки — те же, что гоняет CI на каждый push:

```bash
cd backend  && cargo fmt --check && cargo clippy --all-targets && cargo test
cd frontend && pnpm lint && pnpm typecheck && pnpm test && pnpm build
cd e2e      && ./run.sh        # настоящий Angie + pebble, нужен Docker
```

## Статус

Функционально готов и работает на реальном железе. Выпуск сертификата проверяется на живом
Angie + [pebble](https://github.com/letsencrypt/pebble) на каждый push в CI, а сама панель
развёрнута и проверена на NanoPi R6S (Armbian/Debian 13, arm64): установка, миграции,
apply, реальный выпуск сертификата через DNS-01 reg.ru и планировщик проверок, пишущий
настоящие удары.

Это личный проект, который автор использует в проде. Внешнего аудита безопасности не было —
прочитайте [SECURITY.md](SECURITY.md), прежде чем открывать панель в интернет.

## Участие в разработке

Issues и pull requests приветствуются — как гонять проверки и чего ждёт история коммитов,
описано в [CONTRIBUTING.md](CONTRIBUTING.md).

## Лицензия

[MIT](LICENSE).

В `.deb` вендорится [acme.sh](https://github.com/acmesh-official/acme.sh) для DNS-01 через
API провайдеров. Эти скрипты остаются под своей лицензией GPLv3 и вызываются как отдельный
процесс, а не линкуются.
