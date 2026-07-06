# План разработки: веб-конфигуратор reverse-proxy на базе Angie

Рабочее название: **angie-panel** (обсуждается; ниша «angie ui / angie configurator» в open source
на момент анализа свободна — короткий конкурентный повторный обзор сделать на M0).

Аналог nginx-proxy-manager (NPM), но: нативный сервис на Armbian (без Docker), Angie вместо
OpenResty, сертификаты через **встроенный ACME-модуль Angie** (без certbot/lego/acme.sh),
бэкенд на Rust, фронтенд React + shadcn/ui.

Целевое железо: NanoPi R4S LTS (RK3399, arm64, 4 ГБ RAM), Armbian **Debian 12/13**
(bookworm/trixie; Debian 11 не поддерживаем — там polkit 0.105 без JS-правил).

> План прошёл адверсариальное ревью (факт-чекинг по докам/исходникам/пакетам Angie 1.11.8,
> Rust/systemd-ревью, полнота против NPM, security-ревью). Ключевые следствия вшиты в текст.

---

## 1. Позиционирование и ключевые решения

| Решение | Выбор | Почему |
|---|---|---|
| Реверс-прокси | Angie ≥ 1.11 из официального apt-репозитория | arm64-пакеты для Debian 12/13; ACME-модуль вкомпилирован; `acme_http_port` и `/status/http/acme_clients/` появились в 1.11.0 |
| Сертификаты | Встроенный ACME Angie (`acme_client`/`acme`) | Убирает целый класс болей NPM (certbot, «Internal Error», pip). Angie сам получает и продлевает; ни один существующий UI это не использует |
| Способ конфигурирования | Генерация файлов в `/etc/angie/http.d/` + валидация + graceful reload | В OSS Angie нет write-API; reload безопасен (автооткат при ошибке) |
| Бэкенд | Rust: axum + tokio + sqlx (SQLite) | Один статический musl-бинарник, ~10–20 МБ RAM, ноль рантайм-зависимостей |
| Фронтенд | React 19 + Vite + TS + shadcn/ui + Tailwind | Требование проекта; ассеты встраиваются в бинарник (rust-embed) |
| БД | SQLite в `/var/lib/angie-panel/` | БД — источник истины; конфиги — детерминированная проекция БД |
| Привилегии | Панель без root + **тонкий root-хелпер** (oneshot-юнит через polkit) | Единственный компонент с правами — маленький и аудируемый; панель вообще не пишет в /etc |
| Дистрибуция | .deb (cargo-deb) + install.sh, подписанные | .deb ставит бинарник, юниты, polkit-правило; install.sh дополнительно подключает репозиторий Angie |
| Мониторинг | Read-API Angie (`/status/...`) + `api_config_files` | Статус хостов, апстримов, ACME и фактически загруженный конфиг — без агентов |

Чего сознательно НЕТ в v1 (бэклог v2): streams (TCP/UDP; в пакетном angie.conf stream-блок
закомментирован — потребует документированной правки angie.conf), redirect/404-хосты,
access-листы, мультипользовательский режим, аудит-лог, acme_hook с API DNS-провайдеров,
загрузка своих сертификатов, HTTP/3, rate limiting, лог-вьювер, Prometheus-тумблер
(у Angie родной экспорт: `location =/p8s { prometheus all; }` + пакетный prometheus_all.conf).

## 2. Архитектура

```
┌────────────────────────────── NanoPi R4S (Armbian) ──────────────────────────────┐
│                                                                                   │
│  angie.service (root master + workers)                                            │
│    ├── 80/443 (+udp/53 для dns-01, +443 для challenge=alpn)                       │
│    ├── /etc/angie/angie.conf (пакетный, НЕ трогаем)                               │
│    │     └── include /etc/angie/http.d/*.conf   ← файлы кладёт ТОЛЬКО хелпер      │
│    ├── /var/lib/angie/acme/<client>/  ← сертификаты (владеет Angie)               │
│    └── 127.0.0.1:8100 { api /status/; api_config_files on; }                      │
│                                                                                   │
│  angie-panel.service (пользователь angie-panel, БЕЗ прав на /etc)                 │
│    ├── axum: REST API + встроенный React UI (bind: один LAN-IP, порт из toml)     │
│    ├── SQLite /var/lib/angie-panel/panel.db (0600)                                │
│    ├── генератор конфигов (askama) → /var/lib/angie-panel/staging/                │
│    └── D-Bus StartUnit → хелпер-юниты (polkit-правило: только эти два юнита)      │
│                                                                                   │
│  angie-panel-configtest.service / angie-panel-apply.service (root, oneshot)      │
│    └── ExecStart=/usr/bin/angie-panel helper <mode>  (фиксированный argv)         │
│        валидация → снапшот → атомарный sync staging→http.d → angie -t → reload    │
└───────────────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Модель привилегий: панель + root-хелпер

Честная формулировка threat model (в доку): запись в `http.d` = root-эквивалент через сам Angie
(error_log/root/proxy_pass unix: в конфиге исполняет root-master). Поэтому панель **не пишет
в /etc вообще** — граница доверия перенесена в маленький root-хелпер:

- `angie-panel.service`: пользователь `angie-panel`, полный харденинг
  (`ProtectSystem=strict`, `ReadWritePaths=/var/lib/angie-panel`, `NoNewPrivileges=yes`,
  `CapabilityBoundingSet=`, `SystemCallFilter=@system-service`, `RestrictAddressFamilies=
  AF_UNIX AF_INET AF_INET6`). sudo не используется нигде (NNP его ломает — и не нужен).
- Хелпер — подкоманда того же бинарника, запускается ТОЛЬКО как root-oneshot-юнит с
  фиксированным ExecStart (панель не контролирует argv). Два юнита:
  `...-configtest.service` (только валидация staging) и `...-apply.service` (полный пайплайн).
- Panel → systemd: zbus (D-Bus `StartUnit`) + polkit-правило в `/etc/polkit-1/rules.d/`:
  `subject.user == "angie-panel" && verb == "start" && unit in [два наших юнита]`.
  Требует polkit ≥ 0.106 → Debian 12+. В .deb: `Depends: angie (>= 1.11), dbus, polkitd`.
- Хелпер сам делает `systemctl reload angie` (он root) — отдельного polkit-права на
  angie.service панели не нужно.
- Результат хелпера: ExecMainStatus по D-Bus + JSON-отчёт в
  `/var/lib/angie-panel/apply-result.json` (stderr angie -t, хвост error.log, статус reload).
- Self-check панели при старте: D-Bus доступен, polkit-правило работает (пробный
  CheckAuthorization), staging записываем, версия Angie ≥ 1.11 и ACME-модуль есть
  (`angie -V`) — иначе баннер с точной командой починки, кнопка Apply отключена.

Права на файлы: `/var/lib/angie-panel` 0700 (кроме `public/` 0755 — там HTML default-site,
его читают воркеры Angie), panel.db 0600, бинды/порты панели — в `/etc/angie-panel.toml`
(root-owned, панель читает; настройки, влияющие на привилегии, не живут в БД, которую панель
же и обслуживает).

**Почему `angie -t` нельзя было запускать от пользователя панели** (проверено по исходникам):
в test-режиме Angie открывает на запись все error_log/access_log из конфига, создаёт pid-файл
(`ngx_create_pidfile` при `ngx_test_config`), а ACME-модуль на этапе загрузки конфига создаёт
каталоги 0700 и генерирует ключи в `/var/lib/angie/acme/`. Валидация — только от root
(бонус: тест от root заранее создаёт state и ключи новых acme_client до reload).

### 2.2 Пайплайн применения изменений (главный анти-NPM момент)

Панель (unprivileged):
1. Генерация полного набора файлов из БД в `staging/` + **staging-angie.conf** — копия
   пакетного angie.conf с include, переписанным на staging (валидация до касания продакшена).
2. Diff staging ↔ текущие файлы — страница «Apply» в UI. Preview фиксирует ревизию БД;
   apply с устаревшей ревизией отклоняется. Весь пайплайн — под mutex + маркер
   «apply in progress» (recovery-проверка при старте панели).

Хелпер (root, oneshot):
3. **Линтер-allowlist по сгенерированным файлам** (см. §7): только разрешённые директивы,
   запрет load_module/include/error_log с произвольными путями/proxy_pass на loopback
   management-порты и т.п. Это то, что делает «панель не root» правдой.
4. `angie -t -c staging-angie.conf -e stderr` — **до** касания живого каталога. Ошибка →
   stderr парсится (`файл:строка` → конкретный хост), в UI подсвечивается виновник +
   кнопка «отключить этот хост и применить остальное».
5. Снапшот-манифест текущего состояния http.d (список файлов + хэши + содержимое) в
   `backups/<timestamp>/`, ротация N последних. Rollback = привести каталог точно к
   манифесту, включая удаление добавленных файлов.
6. Атомарный sync: temp-файл **внутри** http.d (имя `.angie-panel.*.tmp` — glob `*.conf` его
   не видит), fsync, same-dir rename, fsync каталога. НЕ rename между каталогами: под
   ProtectSystem=strict это отдельные bind-mounts → EXDEV; плюс SD-карта — fsync обязателен.
7. Контрольный `angie -t` на живом конфиге → `systemctl reload angie`.
8. Верификация: поллинг `/status/angie` (generation++, свежий load_time) с таймаутом ~10 с —
   SIGHUP асинхронен. `angie -t` не биндит сокеты, поэтому конфликты портов всплывают только
   тут; причина — в хвосте `/var/log/angie/error.log`, хелпер кладёт его в отчёт для UI.
9. При любом сбое — откат по манифесту + reload + полный текст ошибки в UI.

Детект правок мимо панели: заголовок `# MANAGED BY angie-panel v<generator_ver> hash:<sha256>`
(хэш по телу без заголовка) + сверка с `/status/angie/` при `api_config_files on;` — видно и
чужие файлы, и «на диске поменяли, но не перечитали». После апгрейда панели старый вывод
матчится по снапшоту → баннер «рекомендуется re-apply», а не ложная тревога «ручная правка».
Чужие файлы в http.d панель не трогает и показывает списком.

Пакетный `/etc/angie/http.d/default.conf` (welcome-страница + /status на :80) — это
dpkg-conffile: postinst с согласия пользователя удаляет его (удаление переживает апгрейды
angie), его роль берут на себя 00-panel.conf и 05-default.conf.

## 3. Модель данных (SQLite, sqlx embedded migrations с M0)

```
proxy_hosts   id, domains(JSON), forward_scheme(http|https), forward_host, forward_port,
              websockets_upgrade, block_exploits, cache_assets, http2,
              force_ssl, hsts, hsts_subdomains, trust_forwarded_proto,
              certificate_id(NULL=без TLS, FK RESTRICT), locations(JSON), advanced_snippet,
              enabled, created_at, updated_at
certificates  id, name(имя acme_client, ^[a-z0-9_]{1,32}$, immutable после создания),
              domains(JSON — АВТОРИТЕТНЫЙ состав SAN), challenge(http|dns|alpn),
              key_type(ecdsa|rsa), email, staging(bool, per-cert), status_cache(JSON),
              created_at        -- поля eab_kid/eab_hmac зарезервированы (EAB уже в master
                                -- Angie, параметр eab=, ожидается в ближайшем релизе)
settings      key, value   (default_site: 404|444|redirect|html; default_site_redirect_url;
              acme_email; ipv6_enabled(auto-detect при установке); resolver_override)
users         id, email, password_hash(argon2id), created_at   (v1 — один админ)
apply_history id, timestamp(UTC epoch), db_revision, diff, result, report(JSON)
```

Правила целостности (server-side, zod на клиенте — не защита):
- Домен (после idna-нормализации) принадлежит максимум одному включённому хосту → 409 с
  именем конфликтующего хоста. Wildcard/exact пересечение допустимо (точное имя побеждает —
  информационная заметка в UI).
- `DELETE /certificates/:id` при наличии ссылающихся хостов → 409 со списком; сначала
  отвязать. Каталог `/var/lib/angie/acme/<name>/` после удаления остаётся (владеет Angie) —
  задокументировать + root-хелпер-команда очистки.
- Все timestamps — UTC epoch; рендер в таймзоне браузера.

### Модель «сертификат ↔ хосты» (важно, тут была бы граната)

`certificates.domains` — авторитетный состав SAN. Генератор для каждого сертификата создаёт
**скрытый служебный server-блок** (в 10-acme.conf) с `server_name <cert.domains>` и
`acme <name>;`. Реальные proxy-хосты директиву `acme` НЕ несут — только
`ssl_certificate $acme_cert_<name>` (переменные глобальны в http-контексте).

Следствия: enable/disable/удаление/правка доменов хоста НИКОГДА не меняет SAN → нет
перевыпусков от тумблеров → нет сжигания rate-limit Let's Encrypt (50 сертификатов/домен/нед).
Привязка сертификата к хосту — чистая проверка покрытия (включая wildcard-матч).
«Добавить домен в сертификат X (будет перевыпуск)» — отдельное подтверждаемое действие.
Сертификат без хостов остаётся валидным и продлевается. По умолчанию UI предлагает
«один хост = один сертификат», общий сертификат — осознанный выбор.

## 4. Генерация конфигов Angie

Раскладка — **плоская**: пакетный include `/etc/angie/http.d/*.conf` не рекурсивный,
подкаталоги не подхватываются.

```
/etc/angie/http.d/
  00-panel.conf     # resolver <из /etc/resolv.conf | override из настроек>;
                    # proxy_cache_path ... keys_zone=assets:10m (для cache_assets);
                    # server 127.0.0.1:8100 { api /status/; api_config_files on; }
  05-default.conf   # default_server :80 и :443: ssl_reject_handshake on; (без dummy-сертов!)
                    # режимы 404|444|redirect|html (HTML — /var/lib/angie-panel/public/)
  10-acme.conf      # на каждый сертификат: acme_client <name> <directory-url>
                    #   [challenge=dns|alpn] [key_type=…] [email=…] [enabled=off];
                    # + скрытый server-блок с server_name <domains> и acme <name>;
  20-host-<id>-<slug>.conf   # по одному файлу на proxy-хост
/usr/share/angie-panel/snippets/   # block-exploits.conf, cache-assets.conf —
                    # включаются absolute-путём, принадлежат пакету (апгрейд обновляет,
                    # детектор дрейфа их игнорирует)
```

Шаблон хоста (askama; все подстановки — через строгие типы, см. §7):

```nginx
upstream host_<id> { zone host_<id> 64k; server 192.168.1.10:8123; }  # zone → метрики
                                                        # /status/http/upstreams + путь к
                                                        # health-checks/балансировке в v2
server {                                # отдельный server для force_ssl-редиректа:
    listen 80; listen [::]:80;          # безусловный return 301 https://$host$request_uri;
    server_name example.com;            # исключение для /.well-known НЕ нужно: http-01
    return 301 https://$host$request_uri;  # перехватывается на фазе POST_READ до return
}
server {
    listen 443 ssl; listen [::]:443 ssl;   # [::] — по флагу ipv6_enabled
    http2 on;
    server_name example.com;
    status_zone host_<id>;
    ssl_certificate     $acme_cert_<name>;
    ssl_certificate_key $acme_cert_key_<name>;
    # hsts → add_header Strict-Transport-Security (+ preload не ставим);
    # block_exploits/cache_assets → include /usr/share/angie-panel/snippets/...
    location / {
        proxy_pass http://host_<id>;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;   # trust_forwarded_proto=true →
        # websockets → proxy_http_version 1.1 + Upgrade/Connection    прокидываем входящий
    }
}
```

Семантика `enabled=false`: файл хоста просто не генерируется, трафик падает в default_server;
SAN сертификата не затрагивается (см. §3); на дашборде хост показан как выключенный.

Окно первого выпуска: `$acme_cert_<name>` пуст до первого получения сертификата (ключ
доступен сразу) — `angie -t` проходит, но TLS-хендшейк падает. Поэтому state machine:
пока `/status/http/acme_clients/<name>` ≠ `certificate: valid`, хост рендерится HTTP-only
(без 443 и без редиректа), UI показывает «HTTPS активируется после выпуска»; при переходе
в valid панель предлагает/делает re-apply. Ошибки TLS вместо сайта — не наш UX.

## 5. Сертификаты (встроенный ACME Angie)

**HTTP-01 (по умолчанию):** с Angie 1.11 отдельный server на :80 не нужен (`acme_http_port`).
Визард: домены → предвалидация (A/AAAA указывает на внешний IP хоста; **если есть AAAA, а
ipv6 выключен — явное предупреждение: LE предпочитает v6 и выпуск упадёт**) → acme_client →
apply → поллинг статуса.

**TLS-ALPN (challenge=alpn, Angie 1.11+):** выпуск целиком через 443 — для провайдеров,
блокирующих 80-й порт (частый кейс домашних инсталляций). Один параметр в конфиге,
третья опция в визарде.

**DNS-01 (wildcard):** Angie сам отвечает на DNS-валидацию (UDP/53, сокет открывает
root-master на этапе загрузки конфига — привилегированный порт ок).
Визард для `*.example.com`:
1. Показывает записи для DNS-провайдера: `_acme-challenge.example.com. NS ns.example.com.`
   + `ns.example.com. A <внешний IP>`. NS-делегирование работает только на порт 53
   (свойство DNS); `acme_dns_port <порт>` полезен только с внешним DNAT 53→порт.
2. «Проверить делегирование» — hickory-resolver итеративно от родительской зоны напрямую к
   авторитетным NS, **минуя системный резолвер** (на R4S часто AdGuard/dnsmasq — кэш и
   фильтры дают ложные результаты). Без прохождения проверки выпуск не запускаем
   (бережём rate-limit).
3. Перед генерацией `acme_dns_port` — проверка, не занят ли :53 локальным резолвером;
   если занят — не отбираем молча, а объясняем варианты (см. Риски).
4. Security-заметки в UI/доке: наружу открывать UDP/53 только для нужд валидации
   (амплификация/сканирование), Angie отвечает только на `_acme-challenge`-запросы.

Продление — забота Angie (renew_before_expiry=30d, ретраи 2ч). API `/status/http/acme_clients/`
отдаёт только state/certificate/details/next_run — **ни notAfter, ни SAN там нет**, поэтому
срок истечения панель получает локальным TLS-хендшейком к 127.0.0.1:443 с нужным SNI
(rustls, верификация не нужна — только парсинг notAfter/SAN), а state=failed/expired — триггеры
алертов в UI.

Готовые UI-действия из параметров acme_client: «Продлить сейчас» (временный `renew_on_load`
+ apply), «Приостановить» (enabled=off — state сохраняется), импорт существующего
ACME-аккаунта (`account_key=` — миграция с certbot/acme.sh).

**LE staging — per-certificate** (не глобально): `staging=true` рендерит клиента под
производным именем `<name>_stg` (state-каталоги prod/staging не смешиваются), в UI —
постоянный бейдж; переключение staging→prod = новый клиент → чистый перевыпуск.

Ограничения v1 (честно в UI): один acme_client = один сертификат; wildcard — только dns-01;
regex server_name не участвуют; EAB (ZeroSSL и т.п.) — после ближайшего релиза Angie.

## 6. REST API и фронтенд

```
POST /api/auth/login|logout             GET  /api/system/status   (angie: версия, ACME-модуль,
GET/POST/PUT/DELETE /api/hosts[/:id]         generation, юнит; self-check полки/D-Bus)
POST /api/hosts/:id/enable|disable      GET/POST/DELETE /api/certificates[/:id]
GET  /api/certificates/:id/status       POST /api/certificates/precheck (A/AAAA | делегирование)
POST /api/certificates/:id/renew|pause  GET  /api/apply/preview   (diff + db_revision)
POST /api/apply                         GET  /api/apply/history
GET/PUT /api/settings                   GET  /api/dashboard
```

Сессии: cookie (tower-sessions, SQLite-store), HttpOnly + SameSite=Lax (+Secure/__Host- на
HTTPS), ротация session id на логине, idle+absolute timeout. CSRF-токен на все мутации.
CORS: same-origin only — заголовки ACAO не выдаём вовсе (урок CVE-2025-50579 у NPM).
Host-header allowlist (защита от DNS rebinding). CSP `default-src 'self'` + frame-ancestors
'none'. Diff и stderr Angie рендерятся только как escaped text (никакого
dangerouslySetInnerHTML — это привязать review-правилом). Rate-limit на /login.
argon2id с параметрами OWASP (m≈19MiB,t=2,p=1) в spawn_blocking + глобальный семафор.
Из utoipa-OpenAPI генерируется TS-клиент — типы сквозные.

**Фронтенд:** Vite + React 19 + TS, shadcn/ui + Tailwind, TanStack Query + Router,
react-hook-form + zod. Страницы: Dashboard, Proxy Hosts (таблица + модал
Details/SSL/Locations/Advanced — привычный NPM-флоу), Certificates (+визарды), Apply
(diff + история), Settings, Login/Setup. Тёмная тема, i18n (ru/en).

## 7. Безопасность (главное)

Уровень 1 — **строгая валидация полей** (allowlist, отклонение, не экранирование):
domains → punycode + `^(\*\.)?([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$`;
forward_host → `IpAddr` ИЛИ строгий hostname (никаких URL/портов/путей внутри);
порты → u16; scheme → enum; location.path → `^/[A-Za-z0-9/_.\-~%]*$`, regex/named locations
в v1 запрещены; rewrite-цель — тот же charset + enum флагов; cert name →
`^[a-z0-9_]{1,32}$` (интерполируется в имя переменной!). Пример атаки, которую это режет:
`forward_host = "1.2.3.4:80; } location /fs { root /; autoindex on; "` — «валидный для
angie -t» конфиг и есть атака, поэтому `angie -t` — не защита от инъекций.

Уровень 2 — **линтер-allowlist в root-хелпере по готовым файлам** (см. §2.2 шаг 3):
hard-deny `load_module`, `include` вне /usr/share/angie-panel/snippets, `error_log`/
`access_log`/`root`/`alias` с произвольными путями, `autoindex`, perl/njs/js_*,
`proxy_pass` на unix:-сокеты и loopback management-порты (8100, порт панели, 127.0.0.0/8,
::1, link-local — без явного override).

**advanced_snippet** (то же для per-location snippet): по умолчанию выключен; включается
только root-owned флагом `/etc/angie-panel.toml: allow_advanced_snippets=true` (атакующий
через веб его не выставит). Даже включённый — проходит линтер уровня 2 (разрешён курируемый
набор: add_header, proxy_set_header, proxy_*-тюнинг, gzip, client_max_body_size, ...;
запрещён выход из контекста `}`). В доке прямо: включённые сниппеты = root-эквивалент.

Setup-flow: одноразовый токен ≥128 бит (256), TTL 24ч, файл
`/var/lib/angie-panel/setup-token` (0600) + journal; эндпоинт setup строго требует токен
(никакого «первый пришёл — тот и админ»: классическая гонка Portainer на LAN), rate-limit,
constant-time сравнение. Loopback-only до клейма НЕ делаем: установка на headless SBC
идёт с ноутбука по LAN — защита именно в токене (получить его = иметь доступ к файлу на
устройстве или к journald). Восстановление доступа:
`sudo angie-panel reset-password` — новый setup-токен без потери данных (иначе топ-issue
«удалил panel.db со всеми хостами»).

Bind: один явный LAN-IP, выбранный при установке (не 0.0.0.0); install.sh определяет
WAN-интерфейс (R4S — часто шлюз!) и отказывается/громко предупреждает при биндинге в WAN.
Рекомендованный рецепт в доке — опубликовать панель через сам Angie (proxy-хост + сертификат
+ IP-allowlist), plain-HTTP LAN-режим честно описан как сниффаемый.

Прочее: бэкапы/diff/история могут содержать секреты из сниппетов → /var/lib/angie-panel 0700,
ротация реально удаляет старые снапшоты, экспорт помечен как чувствительный, импорт проходит
ПОЛНЫЙ пайплайн валидации (уровни 1+2). Все внешние команды — argv без shell. install.sh и
.deb — подписаны (+подпись apt-репо), в доке — проверка перед запуском.

## 8. Установка и пакетирование

**install.sh**: preflight (arm64/x86-64; Debian 12/13 — читаем и ID, и ID_LIKE из
/etc/os-release: Armbian обычно отдаёт ID=debian, но перестраховка нужна + проверка HTTP-кода
репозитория; `ss -tlnp`-проверка занятости 80/443/панельного порта/8100; NTP активен —
у R4S нет RTC, со сбитыми часами ACME/TLS падают загадочно) → репозиторий Angie →
`apt install angie` (опц. angie-console-light) → `angie-panel.deb` → вывод setup-URL+токена.

**.deb (cargo-deb)**: бинарник (panel+helper), три юнита, polkit-правило,
/usr/share/angie-panel/snippets, /etc/angie-panel.toml. postinst: пользователь angie-panel,
каталоги, предложение убрать пакетный default.conf. Depends прописать руками (angie >= 1.11,
dbus, polkitd) — авто-детект для статического бинарника пуст. При апгрейде панели postinst
Angie не трогает: первый re-apply — по кнопке пользователя.

**Сборка:** GitHub Actions: pnpm build → cargo (cross или cargo-zigbuild)
`aarch64-unknown-linux-musl` + `x86_64-...-musl` → cargo-deb → подписанный релиз.
Крипто: rustls с провайдером **ring** (default-фичи выключить — aws-lc-rs капризен на
musl-кроссе); sqlx `runtime-tokio` + bundled sqlite; `cargo tree` в CI — гейт на
openssl-sys/native-tls. SQLite: WAL, busy_timeout, synchronous=NORMAL, запись через одно
соединение (заодно меньше износ SD).

## 9. Тестирование

- Golden-тесты генератора (фикстура БД → ожидаемые .conf), включая negative-кейсы инъекций
  из §7 (крафтовые domain/forward_host/path не ломают структуру директив).
- Тесты линтера-allowlist (уровень 2) на корпусе «злых» конфигов.
- Integration: docker (debian + Angie, локально/CI; сам продукт Docker не требует) — полный
  apply-пайплайн ПОД systemd-юнитом с харденингом (EXDEV-класс ошибок ловится только так),
  polkit-путь, rollback, детект дрейфа.
- ACME e2e: контейнер с pebble (тестовый ACME-сервер) — цикл http-01/alpn и dns-01 против
  реального Angie.
- Смоук на NanoPi R4S по SSH (deploy-скрипт); Frontend: vitest + Playwright.

## 10. Дорожная карта

| Этап | Содержимое | Выход / критерий |
|---|---|---|
| **M0. Каркас и фундамент привилегий** | Repo, CI, cross-сборка; axum + embedded React; auth + setup-токен + reset-password; sqlx-миграции; .deb + 3 юнита + polkit; **хелпер end-to-end (configtest через D-Bus)**; self-check'и; install.sh с preflight | Панель ставится на R4S; из UI можно запустить валидацию конфига через root-хелпер |
| **M1. Хосты и apply** | CRUD proxy-хостов (все тумблеры, locations), генератор + линтер + staging-валидация + манифест-снапшоты + rollback; изоляция ошибок (файл:строка → хост); default site; детект дрейфа | Хост создаётся из UI и работает; кривой сниппет не блокирует остальные изменения |
| **M2. Сертификаты** | HTTP-01 + ALPN: acme_client-генерация, скрытые server-блоки, предвалидация DNS, state machine первого выпуска, статусы из /status + notAfter хендшейком. Затем DNS-01: визард делегирования с итеративной проверкой | HTTPS-хост с автопродлением; wildcard через dns-01; ошибки выпуска видны словами |
| **M3. Дашборд** | /status-поллинг (зоны хостов, upstream-зоны, коннекты, ACME), api_config_files-сверка, история apply, алерты (не продлился, failed, дрейф) | Живой дашборд; панель замечает чужие правки и проблемы продления |
| **M4. Релиз v1.0** | Экспорт/импорт (JSON, через полный пайплайн валидации) — топ-3 запрос к NPM; докуменация RU/EN; подпись артефактов; e2e на устройстве; конкурентный повторный обзор ниши | Публичный релиз |

Порядок жёсткий: M0 доказывает модель привилегий (самый рискованный элемент), M1 — фундамент
для M2; сертификатные флоу ложатся на готовый apply-пайплайн.

## 11. Риски и открытые вопросы

- **:53 занят локальным резолвером** (AdGuard/dnsmasq на R4S — типично): dns-01 требует UDP/53
  для NS-делегирования. Варианты: DNAT снаружи 53→acme_dns_port; выделенный IP; отказ от
  dns-01 в пользу alpn там, где wildcard не нужен. Визард не отбирает порт молча.
- **Поведение `angie -t` от root при новом acme_client** (создание каталогов/ключей на этапе
  теста staging-конфига): проверить на M0 — не оставляет ли тест staging-валидации мусорный
  state; при необходимости — отдельный acme_client_path для staging-теста.
- **resolver для acme_client** обязателен; из /etc/resolv.conf может прийти фильтрующий
  локальный DNS → резолвинг ACME CA ломается. Настройка resolver_override (1.1.1.1/9.9.9.9)
  видна в Settings.
- **Console Light рядом** (опция install.sh): чисто статический мониторинг, не конфликтует;
  наш /status-сервер на 8100 переиспользуется его location-блоком при желании.
- **Апгрейды Angie**: сверять changelog (EAB появится — добавить в UI; поведение acme_http_port
  менялось в 1.11.x). `/status/angie` build/version — как capability probe.
- **Наименование проекта** — выбрать до публичного релиза (занять «angie ui/configurator»).
