# Установка и первый запуск / Installation & first run

> RU ниже, English below.

## Русский

### Требования

- **Железо:** arm64 (NanoPi R4S, Raspberry Pi 4/5, Orange Pi 5, …) или x86-64.
  Официальные пакеты Angie есть только для 64-бит; 32-битные (armhf) не поддерживаются.
- **ОС:** Armbian / Debian 12 (bookworm) или 13 (trixie). Debian 11 не поддерживается
  (там polkit 0.105 без JS-правил).
- Доступ в интернет для установки Angie из официального репозитория.

### Быстрая установка

```sh
# скачайте install.sh из релиза и проверьте подпись (см. ниже), затем:
sudo ./install.sh angie-panel_<версия>_arm64.deb
```

`install.sh` делает:

1. **Префлайт** — архитектура, версия ОС (читает `ID`/`ID_LIKE` из `/etc/os-release`),
   синхронизация часов (NTP — у R4S нет RTC, при сбитых часах ACME/TLS падают),
   свободны ли порты 80/443.
2. Подключает официальный репозиторий Angie и ставит `angie` (+ опционально
   `angie-console-light`).
3. Ставит `angie-panel.deb` (бинарник, systemd-юниты, polkit-правило, сниппеты, tmpfiles).
4. Предлагает удалить пакетный `default.conf` (панель управляет дефолт-сайтом сама).
5. **Выбор адреса привязки** — показывает LAN-адреса, определяет WAN-интерфейс и
   **отказывается** привязываться к нему без явного подтверждения (R4S часто — сам роутер).
6. Запускает панель и печатает URL и одноразовый **setup-токен**.

### Первый вход

Дефолтного логина/пароля нет — это осознанное решение (учимся на дырах NPM).

1. Возьмите токен из вывода install.sh, либо позже:
   ```sh
   sudo cat /var/lib/angie-panel/setup-token      # файл 0600
   # или
   sudo journalctl -u angie-panel | grep "setup token"
   ```
2. Откройте `http://<адрес>:8080/setup`, введите **токен + свой email + пароль** →
   создаётся администратор.
3. Забыли пароль? `sudo angie-panel reset-password` печатает новый токен без потери данных.

### Как безопасно опубликовать панель наружу

Панель по умолчанию слушает один LAN-адрес по HTTP. Не открывайте её в интернет напрямую.
Рекомендованный способ — опубликовать через сам Angie: создайте proxy-хост на панель
(`http://127.0.0.1:8080`), привяжите сертификат и ограничьте доступ по IP (access-лист) или
базовой аутентификацией. Так трафик к панели идёт по HTTPS.

### Проверка подписи артефактов

Релизы подписаны GPG (если у мейнтейнера настроен ключ). Рядом с каждым файлом лежит
`<файл>.asc`, а публичный ключ — `angie-panel-signing-key.asc`:

```sh
gpg --import angie-panel-signing-key.asc
gpg --verify angie-panel_<версия>_arm64.deb.asc angie-panel_<версия>_arm64.deb
sha256sum -c SHA256SUMS.txt
```

---

## English

### Requirements

- **Hardware:** arm64 (NanoPi R4S, Raspberry Pi 4/5, Orange Pi 5, …) or x86-64. Official
  Angie packages are 64-bit only; 32-bit (armhf) is unsupported.
- **OS:** Armbian / Debian 12 (bookworm) or 13 (trixie). Debian 11 is unsupported (polkit
  0.105 lacks JS rules).

### Quick install

```sh
sudo ./install.sh angie-panel_<version>_arm64.deb
```

The installer runs preflight checks (arch, OS, NTP, free ports), sets up the official Angie
repo, installs Angie + the panel, lets you pick a bind address (refusing the WAN interface
without explicit confirmation), and prints the setup URL + one-time token.

### First login

There is **no default password** by design. Read the one-time token
(`sudo cat /var/lib/angie-panel/setup-token` or `journalctl -u angie-panel`), open
`http://<address>:8080/setup`, and create the admin with the token + your own email/password.
Recover a lost password with `sudo angie-panel reset-password`.

### Exposing the panel safely

The panel binds one LAN address over HTTP by default. Do not expose it to the internet
directly — publish it *through Angie* (a proxy host to `http://127.0.0.1:8080` with a
certificate and an IP allowlist) so traffic is HTTPS.

### Verifying artifacts

```sh
gpg --import angie-panel-signing-key.asc
gpg --verify angie-panel_<version>_arm64.deb.asc angie-panel_<version>_arm64.deb
sha256sum -c SHA256SUMS.txt
```
