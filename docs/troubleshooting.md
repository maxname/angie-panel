# Диагностика / Troubleshooting

## Русский

### Панель не запускается / не открывается

```sh
systemctl status angie-panel
journalctl -u angie-panel -e
```
- Проверьте адрес привязки и порт в `/etc/angie-panel.toml` (`bind_addr`, `port`).
- Панель слушает LAN-адрес — с ноутбука откройте `http://<этот-адрес>:8080`.

### «Применить» падает с ошибкой валидации

Панель показывает точный stderr от `angie -t` с указанием файла и строки. Обычно это ошибка в
пользовательском сниппете (advanced config) конкретного хоста — панель подсвечивает
виновника. Исправьте поле или отключите хост и примените снова.

### «Применить» падает на reload

`angie -t` не биндит сокеты, поэтому конфликт портов (кто-то занял 80/443/8100) виден только
на reload. Панель показывает хвост `/var/log/angie/error.log` и **откатывается** на прошлый
рабочий конфиг. Освободите порт и примените снова.

Если reload вообще не срабатывает — проверьте polkit-правило и D-Bus:
```sh
ls /usr/share/polkit-1/rules.d/10-angie-panel.rules
systemctl status angie-panel-apply.service
```
На Debian без polkitd используйте sudoers-фолбэк (см. `packaging/README.md`).

### Сертификат не выпускается

Статус на дашборде и в разделе «Сертификаты» показывает `state`/`certificate` из
`/status/http/acme_clients/`. Частые причины:

- **HTTP-01:** домен не указывает на этот хост (проверьте A/AAAA), или порт 80 недоступен
  снаружи. Если есть AAAA-запись, а IPv6 на хосте не работает — Let's Encrypt предпочтёт IPv6
  и упадёт; включите IPv6 или уберите AAAA.
- **DNS-01:** не настроено NS-делегирование `_acme-challenge`, либо UDP/53 закрыт снаружи,
  либо порт 53 занят локальным резолвером (AdGuard/dnsmasq — типично на R4S).
- **Часы сбиты** (у R4S нет RTC) — ACME/TLS падают загадочно. Проверьте:
  `timedatectl show -p NTPSynchronized`.
- **resolver** не настроен — `acme_client` не может резолвить адрес CA. Панель берёт его из
  `/etc/resolv.conf`; при фильтрующем локальном DNS задайте override в Настройках.

### HTTPS не включается после выпуска

Обычно включается автоматически в течение ~30 секунд (реконсайлер). Если нет:
- есть несохранённые правки в БД (реконсайлер не трогает конфиг, пока есть pending-изменения —
  примените их вручную);
- обнаружен дрейф (кто-то правил файл в `/etc/angie/http.d` руками) — дашборд покажет алерт;
  примените заново, чтобы восстановить управляемый файл.

### Предупреждение о дрейфе / чужих файлах

Панель управляет только файлами со своим заголовком `# MANAGED BY angie-panel`. Если файл
поменяли на диске — это «дрейф» (переприменение восстановит его). Чужие файлы в `http.d`
панель не трогает, но показывает их наличие.

### Полезные команды

```sh
angie -t                                  # проверить конфиг
angie -T | less                           # показать merged-конфиг
curl -s http://127.0.0.1:8100/status/angie | jq   # статус Angie
sudo angie-panel reset-password           # новый setup-токен
```

---

## English (summary)

- **Panel won't start / open:** `journalctl -u angie-panel -e`; check `bind_addr`/`port` in
  `/etc/angie-panel.toml`; open `http://<lan-address>:8080`.
- **Apply fails validation:** the panel shows the exact `angie -t` stderr with file:line —
  usually a bad advanced snippet on a host. Fix or disable the host and re-apply.
- **Apply fails on reload:** port conflicts surface only at reload (`angie -t` doesn't bind);
  the panel shows the `error.log` tail and **rolls back**. Free the port and re-apply. If
  reload never fires, check the polkit rule / `angie-panel-apply.service` (sudoers fallback in
  `packaging/README.md`).
- **Cert not issuing:** check the status pill (`/status/http/acme_clients/`). HTTP-01 → domain
  must point here + port 80 reachable (mind IPv6/AAAA); DNS-01 → NS delegation +
  reachable/free UDP/53; clock must be NTP-synced (R4S has no RTC); a `resolver` must be
  configured.
- **HTTPS not activating after issuance:** normally auto-activates within ~30s (reconciler).
  If not, you have pending edits (apply them) or drift (a managed file was hand-edited — the
  dashboard flags it; re-apply to restore).
- Useful: `angie -t`, `angie -T`, `curl 127.0.0.1:8100/status/angie | jq`,
  `sudo angie-panel reset-password`.
