# Сертификаты (встроенный ACME Angie) / Certificates

> Как работают сертификаты в Angie Panel — без certbot, целиком силами Angie.
> How certificates work — no certbot, entirely inside Angie.

## Русский

### Общая идея

Angie умеет получать и продлевать сертификаты по протоколу ACME сам. Панель не запускает
certbot/lego/acme.sh — она лишь **генерирует конфигурацию** `acme_client`, а выпуском,
хранением и продлением занимается Angie. Это убирает целый класс болей NPM (сломанный
certbot, «Internal Error», pip-зависимости).

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

1. Создаёте сертификат (домены, способ проверки), привязываете к хосту, жмёте **Применить**.
2. Панель генерирует `acme_client` + collector, хост пока отдаёт HTTP.
3. Angie выпускает сертификат.
4. **HTTPS включается автоматически** — фоновый реконсайлер замечает `certificate: valid` в
   `/status` и переприменяет конфиг (появляется 443-блок с редиректом). Вручную жать
   «Применить» повторно не нужно.

### Способы проверки (challenge)

- **HTTP-01** (по умолчанию) — Angie отвечает на проверку на порту 80. Домен должен указывать
  A/AAAA-записью на этот хост. С Angie 1.11 отдельный server на :80 не нужен.
- **TLS-ALPN** — выпуск целиком через 443, без порта 80. Полезно, если провайдер блокирует 80.
- **DNS-01** — единственный способ для **wildcard** (`*.example.com`). Angie сам отвечает на
  DNS-запросы валидации на UDP/53. Требуется **делегирование**: в вашей зоне создаётся
  `_acme-challenge.example.com. IN NS <этот-хост>.`, а UDP/53 доступен снаружи. Панель
  показывает нужные записи в визарде. Провайдерных API-интеграций нет (это фича Angie).

### Staging Let's Encrypt

У каждого сертификата есть флаг «staging» — выпуск от тестового CA Let's Encrypt (высокие
лимиты, но **недоверенные** сертификаты). Используйте для отладки, затем переключите на prod.

### Ограничения

- Один `acme_client` = один сертификат; домены объединяются в SAN одного сертификата.
- Wildcard — только dns-01. Regex в `server_name` в сертификат не попадают.
- EAB (ZeroSSL/Google) появится после ближайшего релиза Angie.

---

## English (summary)

Angie issues and renews certificates itself via ACME — the panel does **not** run
certbot/lego/acme.sh; it only generates `acme_client` configuration. Per certificate it emits
an `acme_client` plus a **unix-socket collector server block** (Angie's documented
"domain-collector" pattern) that carries `acme <name>` + the authoritative `server_name` list;
real proxy hosts reference only `$acme_cert_<name>`. This decouples the certificate's SAN from
host toggling (no needless reissuance / rate-limit risk) and breaks the first-issuance
deadlock (`$acme_cert_<name>` is empty until issued, so the collector always drives issuance
while the host's 443 block appears only once the cert is `ready`). **HTTPS activates
automatically** — a background reconciler re-applies once `/status` reports the cert valid.

Challenges: **HTTP-01** (default, port 80), **TLS-ALPN** (via 443, no port 80), **DNS-01**
(required for wildcards; Angie answers validation on UDP/53 and you delegate
`_acme-challenge.<domain>` to this host — no DNS-provider API integration). A per-certificate
**staging** flag uses Let's Encrypt's test CA (untrusted certs, high limits). Certs live in
`/var/lib/angie/acme/<name>/`, owned by Angie.
