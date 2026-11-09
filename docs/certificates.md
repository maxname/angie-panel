# Сертификаты (встроенный ACME Angie) / Certificates

> Как работают сертификаты в Angie Panel — без certbot, целиком силами Angie.
> How certificates work — no certbot, entirely inside Angie.

## Русский

### Общая идея

Angie умеет получать и продлевать сертификаты по протоколу ACME сам. Панель не запускает
certbot/lego — она лишь **генерирует конфигурацию** `acme_client`, а выпуском, хранением и
продлением занимается Angie. Это убирает целый класс болей NPM (сломанный certbot,
«Internal Error», pip-зависимости). Единственное исключение — DNS-01 через API провайдера: для
добавления TXT-записи панель вызывает вкомпилированный `acme.sh` как DNS-хелпер (не как
ACME-клиент — сам сертификат всё равно выпускает Angie; см. раздел про challenge ниже).

Хранилище: `/var/lib/angie/acme/<имя>/` (`account.key`, `certificate.pem`, `private.key`),
владелец — Angie. Панель туда не пишет; статус берёт из API `/status/http/acme_clients/`.

### Как это устроено в конфиге

На каждый сертификат панель генерирует `acme_client` + **collector-блок на unix-сокете**
(документированный Angie паттерн «сервер для сбора имён»):

```nginx
acme_client web https://acme-v02.api.letsencrypt.org/directory email=you@example.com;
server {
    listen unix:/run/angie-panel/acme-web.sock;   # не обслуживает трафик
    server_name app.example.com www.example.com;   # авторитетный список доменов (SAN)
    acme web;                                        # запускает выпуск
}
```

Реальные proxy-хосты директиву `acme` не несут — только `ssl_certificate $acme_cert_web`.
Это даёт две важные вещи:

1. **SAN сертификата не зависит** от того, включён/выключен/привязан ли хост — нет лишних
   перевыпусков и риска упереться в rate-limit Let's Encrypt.
2. **Нет дедлока первого выпуска.** Переменная `$acme_cert_<name>` пуста, пока сертификат не
   выпущен хотя бы раз. Collector с `acme` присутствует всегда → Angie может выпускать; а
   443-блок хоста появляется только когда сертификат `ready`.

### Что видит пользователь

1. Создаёте сертификат (домены, способ проверки) и жмёте **Применить**. Выпуск начинается
   сразу — привязывать сертификат к хосту для этого **не нужно** (за выпуск отвечает
   collector-блок, а не хост).
2. Angie выпускает сертификат; панель генерирует `acme_client` + collector.
3. Привязываете сертификат к прокси-хосту (редактор хоста → вкладка SSL) — это включает HTTPS
   для этого хоста. Сделать это можно до или после выпуска.
4. **HTTPS включается автоматически** — фоновый реконсайлер замечает `certificate: valid` в
   `/status` и переприменяет конфиг (появляется 443-блок с редиректом). Вручную жать
   «Применить» повторно не нужно.

### Способы проверки (challenge)

- **HTTP-01** (по умолчанию) — Angie отвечает на проверку на порту 80. Домен должен указывать
  A/AAAA-записью на этот хост. С Angie 1.11 отдельный server на :80 не нужен.
- **TLS-ALPN** — выпуск целиком через 443, без порта 80. Полезно, если провайдер блокирует 80.
- **DNS-01** — единственный способ для **wildcard** (`*.example.com`). Есть два режима ответа
  на проверку (выбираются в визарде сертификата):
  - **API DNS-провайдера** (рекомендуется, работает за NAT). Панель сама создаёт TXT-запись
    `_acme-challenge` через API вашего провайдера — под капотом мост к `acme.sh` dnsapi
    (12 провайдеров: Cloudflare, Route 53, DigitalOcean, Gandi, deSEC, Namecheap, GoDaddy,
    Vultr, Linode, Porkbun, reg.ru, PowerDNS). Доступы задаются **профилями** на странице
    «DNS-провайдеры» (можно несколько профилей одного провайдера — например, два токена
    Cloudflare). UDP/53 не нужен. Механизм — `acme_hook`: Angie дёргает панель на add/remove,
    панель ходит в API провайдера.
  - **Angie отвечает сам** (NS-делегирование). Angie отвечает на DNS-запросы валидации на
    UDP/53; в вашей зоне создаётся `_acme-challenge.example.com. IN NS <этот-хост>.`, а UDP/53
    доступен снаружи. Панель показывает нужные записи в визарде. Не годится за NAT.

### Staging Let's Encrypt

У каждого сертификата есть флаг «staging» — выпуск от тестового CA Let's Encrypt (высокие
лимиты, но **недоверенные** сертификаты). Используйте для отладки, затем переключите на prod.
Флаг доступен только для Let's Encrypt — у остальных CA тестовой среды в панели нет.

### Центр сертификации (CA)

У каждого сертификата свой CA: **Let's Encrypt** (по умолчанию), **ZeroSSL**, **Google Trust
Services** или **свой ACME-сервер** (любой CA по RFC 8555 — Step CA, Actalis, внутренний PKI).
CA по умолчанию для новых сертификатов и URL каталога своего сервера задаются в
«Настройки» → ACME.

ZeroSSL и Google выдают сертификаты только аккаунтам с **External Account Binding (EAB)**.
Key ID и HMAC-ключ берутся в кабинете CA и вводятся в «Настройки» → «Доступы к центрам
сертификации». HMAC-ключ хранится зашифрованным (как доступы DNS-провайдеров) и обратно не
отдаётся. Без EAB панель не даст создать сертификат у такого CA. EAB передаётся только при
первой регистрации аккаунта ACME.

### Одна учётная запись ACME на все сертификаты

По умолчанию Angie создаёт отдельную учётную запись ACME на каждый `acme_client`. При большом
числе доменов это упирается в лимит Let's Encrypt: **10 новых аккаунтов с одного IP за
3 часа** (`too many new registrations`). Переключатель «Одна учётная запись ACME для всех
сертификатов» в настройках добавляет каждому `acme_client` общий ключ:

```nginx
acme_client web https://acme-v02.api.letsencrypt.org/directory account_key=/var/lib/angie/acme/angie-panel-account.key;
```

Сертификаты остаются отдельными — общий только аккаунт. Если файла нет, Angie создаёт его
сам при загрузке конфига. Путь задаётся в `/etc/angie-panel.toml` (`[angie] acme_account_key`),
а не в веб-интерфейсе: root-хелпер отклонит любой другой `account_key=` в сгенерированном
конфиге. Можно положить туда ключ существующего аккаунта (например, от certbot или acme.sh
в формате PEM).

### Сертификат прямо из proxy-хоста

В редакторе хоста на вкладке SSL есть вариант **«Запросить новый сертификат»**. При сохранении
панель создаёт сертификат на домены хоста (CA по умолчанию, email из настроек, выбранный
способ проверки) и сразу привязывает его. Через API то же самое — поле `new_certificate` в
теле `POST`/`PUT /api/hosts`:

```json
{ "domains": ["app.example.com"], "forward_host": "10.0.0.5", "forward_port": 8080,
  "forward_scheme": "http", "new_certificate": { "challenge": "http" } }
```

### Ограничения

- Один `acme_client` = один сертификат; домены объединяются в SAN одного сертификата.
- Wildcard — только dns-01. Regex в `server_name` в сертификат не попадают.

---

## English (summary)

Angie issues and renews certificates itself via ACME — the panel does **not** run
certbot/lego; it only generates `acme_client` configuration. (For DNS-01 via a provider API it
invokes a bundled `acme.sh` purely as a DNS TXT-record helper through `acme_hook` — Angie still
does the actual issuance.) Per certificate it emits
an `acme_client` plus a **unix-socket collector server block** (Angie's documented
"domain-collector" pattern) that carries `acme <name>` + the authoritative `server_name` list;
real proxy hosts reference only `$acme_cert_<name>`. This decouples the certificate's SAN from
host toggling (no needless reissuance / rate-limit risk) and breaks the first-issuance
deadlock (`$acme_cert_<name>` is empty until issued, so the collector always drives issuance
while the host's 443 block appears only once the cert is `ready`). **HTTPS activates
automatically** — a background reconciler re-applies once `/status` reports the cert valid.

Challenges: **HTTP-01** (default, port 80), **TLS-ALPN** (via 443, no port 80), **DNS-01**
(required for wildcards). DNS-01 has two modes: a **DNS-provider API** — the panel writes the
`_acme-challenge` TXT via your provider through an `acme.sh`-dnsapi bridge (12 providers, including PowerDNS,
configured as named profiles on the DNS Providers page; works behind NAT, no UDP/53) — or
**Angie answers on UDP/53** itself (you NS-delegate `_acme-challenge.<domain>` to this host). A
per-certificate **staging** flag uses Let's Encrypt's test CA (untrusted certs, high limits).
Certs live in `/var/lib/angie/acme/<name>/`, owned by Angie.

Each certificate picks its **CA**: Let's Encrypt (default), ZeroSSL, Google Trust Services, or a
custom RFC 8555 directory URL (Settings → ACME). ZeroSSL and Google need **EAB** credentials
(Settings → Certificate authority credentials; the HMAC key is sealed at rest and write-only).
**One ACME account for all certificates** (Settings → ACME) adds a shared
`account_key=` to every `acme_client`, so many domains no longer hit Let's Encrypt's
10-registrations-per-IP-per-3h limit; the path comes from `[angie] acme_account_key` in the
root-owned `/etc/angie-panel.toml`, and the root helper rejects any other path. A proxy host
can **request a new certificate** for its own domains on save (SSL tab, or `new_certificate`
in the `POST`/`PUT /api/hosts` body).
