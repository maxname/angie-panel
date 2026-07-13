# Установка и настройка Angie на NanoPi R6S (Armbian / Debian 13)

> Пошаговая инструкция, как поставить и оптимально настроить Angie на NanoPi R6S
> под управление Angie Panel. Проверено вживую на устройстве `192.168.8.4`.
> English summary at the bottom.

## Целевое устройство (проверено)

| | |
|---|---|
| Плата | NanoPi R6S (Rockchip RK3588S, 8 ядер, 8 ГБ RAM) |
| ОС | Armbian 26.5.1 «trixie» = Debian GNU/Linux 13 (trixie) |
| Ядро | 6.1.115-vendor-rk35xx, `aarch64` (arm64) |
| Диск | eMMC/SD 57 ГБ (после установки занято ~1.6 ГБ) |
| Angie | **1.12.0** из официального репозитория |

Всё делается под `root` по SSH: `ssh root@192.168.8.4`.

## 0. Предпосылки

Уже присутствуют в Armbian: `curl`, `gpg`, `bash`, `jq`. Проверить:

```bash
for t in curl gpg bash; do command -v "$t" >/dev/null && echo "$t: ok" || echo "$t: MISSING"; done
```

`socat` не нужен: DNS-01 через API провайдера (мост acme.sh dnsapi в панели) ходит по HTTP
через `curl`, а не через standalone-режим acme.sh.

## 1. Официальный репозиторий Angie

Ключ подписи и репозиторий для Debian 13 (`trixie`), формат deb822:

```bash
install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://angie.software/keys/angie-signing.gpg -o /etc/apt/keyrings/angie-signing.gpg

cat > /etc/apt/sources.list.d/angie.sources <<'EOF'
Types: deb
URIs: https://download.angie.software/angie/debian/13
Suites: trixie
Components: main
Signed-By: /etc/apt/keyrings/angie-signing.gpg
EOF

apt-get update
```

Проверка ключа (fingerprint должен быть `EB8E AF3D 4EF1 B1EC F348  65A2 617A B978 CB84 9A76`):

```bash
gpg --show-keys --with-fingerprint /etc/apt/keyrings/angie-signing.gpg
apt-cache policy angie      # Candidate: 1.12.0-1~trixie
```

> Для другого дистрибутива поменяйте путь/кодовое имя: URL вида
> `https://download.angie.software/angie/debian/<VERSION_ID>` + `Suites: <codename>`
> (напр. `12` + `bookworm`). Проверить наличие:
> `curl -sI https://download.angie.software/angie/debian/13/dists/trixie/Release`.

## 2. Установка

```bash
DEBIAN_FRONTEND=noninteractive apt-get install -y angie angie-console-light
```

- **angie** — ядро. Сборка уже включает всё, что нужно панели:
  `--with-http_acme_module` (встроенный ACME), `--with-http_v3_module` (HTTP/3),
  `--with-stream` + `stream_acme` + `stream_ssl` + `stream_ssl_preread` (SNI-роутеры,
  stream-TLS), `--http-acme-client-path=/var/lib/angie/acme`.
- **angie-console-light** — лёгкая веб-панель мониторинга (опционально; читает `/status`).

Проверить сборку:

```bash
angie -V 2>&1 | tr ' ' '\n' | grep -iE 'acme|http_v3|stream'
```

Сервис после установки уже `enabled` + `active`, слушает `:80`, отдаёт `/status`:

```bash
systemctl is-enabled angie && systemctl is-active angie
curl -s http://127.0.0.1/status/angie      # JSON с версией — API для панели
```

## 3. Оптимальная настройка

Пакетный `/etc/angie/angie.conf` уже разумен: `worker_processes auto`,
`worker_rlimit_nofile 65536`, `worker_connections 65536`, `sendfile on`,
`include /etc/angie/http.d/*.conf;` и **закомментированный** блок `stream {}`
(его активирует сама панель при первом стриме/SNI-роутере — не трогайте руками).

`/etc/angie/http.d/default.conf` уже отдаёт `/status/` (только с `127.0.0.1`) — это то,
что нужно дашборду панели.

Единственное важное, чего не хватает из коробки, — **`resolver`** (без него встроенный
ACME не резолвит адрес УЦ, а `proxy_pass` не резолвит хосты по имени). Добавляем его и
несколько полезных дефолтов **в блок `http {}`** (панель управляет только `http.d/` и
`stream.d/`, а не самим `angie.conf`, так что правки безопасны):

```bash
cp -a /etc/angie/angie.conf /etc/angie/angie.conf.orig-$(date +%Y%m%d)

cat > /tmp/angie-tuning.txt <<'EOF'

    # --- provisioning tuning (added for optimal operation + Angie Panel) ---
    server_tokens        off;
    tcp_nopush           on;
    tcp_nodelay          on;
    types_hash_max_size  2048;

    # DNS resolver — required for built-in ACME (reaching the CA) and for
    # proxy_pass to hostnames. Uses the local systemd-resolved stub.
    resolver             127.0.0.53 valid=300s ipv6=off;
    resolver_timeout     5s;

    # Sensible global TLS defaults (per-host certs are managed by the panel).
    ssl_protocols        TLSv1.2 TLSv1.3;
    ssl_session_cache    shared:SSL:10m;
    ssl_session_timeout  1d;
    ssl_prefer_server_ciphers off;
    # --- end provisioning tuning ---
EOF

sed -i '/keepalive_timeout/r /tmp/angie-tuning.txt' /etc/angie/angie.conf
rm -f /tmp/angie-tuning.txt

angie -t && systemctl reload angie
```

> `resolver 127.0.0.53` — локальный стаб systemd-resolved (в Armbian активен). `ipv6=off`
> убирает AAAA-ответы, чтобы ACME/апстримы не спотыкались, если у устройства нет
> IPv6-маршрута. Если systemd-resolved не используется — подставьте адрес роутера или
> публичный резолвер (`1.1.1.1`, `9.9.9.9`).

## 4. Проверка

```bash
angie -t                                         # синтаксис ок
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1/status/angie   # 200
curl -sI --max-time 10 https://acme-v02.api.letsencrypt.org/directory | head -1  # HTTP/2 200
timedatectl | grep -iE 'synchronized|NTP'        # часы синхронизированы — критично для ACME
angie -T | grep -E 'resolver |server_tokens'     # тюнинг виден в эффективном конфиге
```

Ожидаемо: `angie -t` успешен, `/status` = 200, до Let's Encrypt есть HTTP/2 200,
`System clock synchronized: yes`, `NTP service: active`.

## 5. Что дальше — установка панели

Angie готов и оптимально настроен. Следующий шаг — поставить сам Angie Panel (отдельный
`.deb` для arm64: см. [installation.md](installation.md)), который будет управлять
`http.d/`/`stream.d/`, выпускать сертификаты через встроенный ACME и читать `/status`.

Ничего специально доготавливать на стороне Angie не нужно: панель через root-хелпер
(polkit) пишет в `/etc/angie/http.d`, при первом стриме сама активирует блок `stream {}`
в `angie.conf`, а ACME-сертификаты кладёт в `/var/lib/angie/acme` (каталог уже создан
пакетом).

### Замечания по безопасности

- `/status` открыт только с `127.0.0.1` (пакетный дефолт) — оставьте так; панель ходит с
  localhost. Не открывайте наружу.
- `server_tokens off` уже скрывает версию.
- Сервис слушает `:80` на всех интерфейсах (`0.0.0.0`). Если панель публикуется наружу —
  прячьте админку за самой Angie (proxy-хост + сертификат + access-лист), а не напрямую.

---

## English summary

Provisioning Angie on a NanoPi R6S (Armbian 26.5.1 = Debian 13 «trixie», arm64, 8 GB),
verified live:

1. **Repo**: add the Angie signing key to `/etc/apt/keyrings/angie-signing.gpg` and a
   deb822 source `URIs: https://download.angie.software/angie/debian/13`, `Suites: trixie`.
2. **Install**: `apt-get install -y angie angie-console-light`. The build already ships
   `http_acme`, `http_v3`, and the full `stream` module set the panel relies on.
3. **Tune**: the packaged `angie.conf` is already good (auto workers, 65536 conns,
   sendfile, commented `stream {}` that the panel activates). Add a `resolver`
   (`127.0.0.53`, needed for built-in ACME + hostname `proxy_pass`) plus
   `server_tokens off`, `tcp_nopush/nodelay`, and global TLS session defaults inside
   `http {}`. The panel manages `http.d/`/`stream.d/`, not `angie.conf`, so these edits
   are safe. `angie -t && systemctl reload angie`.
4. **Verify**: `/status` returns 200, outbound to `acme-v02.api.letsencrypt.org` is
   reachable (HTTP/2 200), and the clock is NTP-synced (required for ACME).
5. **Next**: install the Angie Panel `.deb` (arm64) — see `installation.md`.
