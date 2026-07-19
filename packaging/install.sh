#!/usr/bin/env bash
#
# angie-panel installer for Armbian / Debian 12-13 (arm64, amd64).
#
# What it does, in order:
#   1. Preflight checks: root, architecture, OS version, NTP sync, busy ports.
#   2. Adds the official Angie apt repository and installs Angie.
#   3. Installs the angie-panel .deb (path in $1, or downloads from releases).
#   4. Optionally removes the packaged Angie default site.
#   5. Lets you pick a safe LAN bind address for the panel (refuses WAN).
#   6. Starts the panel and prints the setup URL and one-time token.
#
# Usage: sudo ./install.sh [path/to/angie-panel.deb]

set -euo pipefail

# ---------------------------------------------------------------------------
# The release version is substituted into this line by the release workflow
# (packaging/install.sh is copied into each release with __PANEL_VERSION__
# replaced by the tag). A checked-out copy keeps the sentinel, so the download
# path below refuses to run and asks for a local .deb instead. {ARCH} is
# substituted at runtime with arm64 or amd64.
PANEL_VERSION="__PANEL_VERSION__"
PANEL_RELEASE_BASE="https://github.com/maxname/angie-panel/releases/download/v${PANEL_VERSION}"
PANEL_DEB_URL_TEMPLATE="${PANEL_RELEASE_BASE}/angie-panel_${PANEL_VERSION}_{ARCH}.deb"

# Fingerprint of the release signing key. THE trust anchor of this script: the
# public key is fetched from the release like everything else, so it proves
# nothing by itself — it is trusted only because its fingerprint matches this
# constant, which reached you with the script (and is published in the README
# and docs/installation.md, so you can compare it there before running this).
#
# Verify it independently:
#   gpg --keyserver hkps://keys.openpgp.org --recv-keys 21773F03FDFC43ED
PANEL_SIGNING_FPR="E81C9989402A5C15B0DD21B921773F03FDFC43ED"
# ---------------------------------------------------------------------------

PANEL_CONF=/etc/angie-panel.toml
TOKEN_FILE=/var/lib/angie-panel/setup-token
DEFAULT_SITE=/etc/angie/http.d/default.conf
DEFAULT_PANEL_PORT=8080
STATUS_PORT=8100
CONFIRM_PHRASE="понимаю риск"

info() { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[внимание]\033[0m %s\n' "$*"; }
die()  { printf '\033[1;31m[ошибка]\033[0m %s\n' "$*" >&2; exit 1; }

# Print processes listening on TCP port $1 (empty output = port is free).
port_listeners() {
    ss -tlnpH "( sport = :$1 )" 2>/dev/null || true
}

# Verify a downloaded .deb against the release's signed checksum list.
#   $1 — path to the .deb, $2 — its filename as listed in SHA256SUMS.txt
#
# The chain: this script pins the signing key's fingerprint -> that key must be
# the one that signed SHA256SUMS.txt -> the .deb must match its entry there.
# Every link fails closed: TLS alone is not evidence about what a mirror or a
# compromised release served us.
verify_panel_deb() {
    local deb_path="$1" deb_name="$2"
    local dir; dir=$(dirname "$deb_path")
    local gnupg_home="$dir/gnupg"

    info "Проверяем подпись и контрольную сумму пакета..."

    if ! curl -fsSL -o "$dir/SHA256SUMS.txt" "${PANEL_RELEASE_BASE}/SHA256SUMS.txt" \
       || ! curl -fsSL -o "$dir/SHA256SUMS.txt.asc" "${PANEL_RELEASE_BASE}/SHA256SUMS.txt.asc" \
       || ! curl -fsSL -o "$dir/signing-key.asc" "${PANEL_RELEASE_BASE}/angie-panel-signing-key.asc"; then
        die "Не удалось скачать файлы проверки (SHA256SUMS.txt, .asc, ключ) из релиза.
Пакет НЕ установлен: без них подлинность загрузки не подтвердить."
    fi

    # Isolated keyring: never touch root's, and leave nothing behind.
    rm -rf "$gnupg_home"
    mkdir -p "$gnupg_home"
    chmod 700 "$gnupg_home"
    if ! GNUPGHOME="$gnupg_home" gpg --batch --quiet --import "$dir/signing-key.asc" 2>/dev/null; then
        die "Не удалось импортировать ключ подписи из релиза. Пакет НЕ установлен."
    fi

    local got_fpr
    got_fpr=$(GNUPGHOME="$gnupg_home" gpg --batch --with-colons --fingerprint 2>/dev/null \
              | awk -F: '/^fpr:/ {print $10; exit}')
    if [ "$got_fpr" != "$PANEL_SIGNING_FPR" ]; then
        die "Отпечаток ключа подписи не совпадает с ожидаемым. Пакет НЕ установлен.
  ожидался: $PANEL_SIGNING_FPR
  получен:  ${got_fpr:-<пусто>}
Это либо подмена, либо смена ключа проекта. Не устанавливайте пакет,
пока не сверите отпечаток с README на github.com/maxname/angie-panel"
    fi

    # --status-fd, а не текст вывода: формулировки gpg зависят от локали, а
    # VALIDSIG несёт отпечаток подписавшего — сверяем именно его.
    if ! GNUPGHOME="$gnupg_home" gpg --batch --status-fd 1 \
            --verify "$dir/SHA256SUMS.txt.asc" "$dir/SHA256SUMS.txt" 2>/dev/null \
            | grep -q "^\[GNUPG:\] VALIDSIG $PANEL_SIGNING_FPR"; then
        die "Подпись SHA256SUMS.txt недействительна или сделана другим ключом.
Пакет НЕ установлен."
    fi

    local expected actual
    expected=$(awk -v f="$deb_name" '$2 == f || $2 == "*" f {print $1; exit}' "$dir/SHA256SUMS.txt")
    if [ -z "$expected" ]; then
        die "В SHA256SUMS.txt нет записи для $deb_name. Пакет НЕ установлен."
    fi
    actual=$(sha256sum "$deb_path" | awk '{print $1}')
    if [ "$expected" != "$actual" ]; then
        die "Контрольная сумма пакета не совпала. Пакет НЕ установлен.
  ожидалась: $expected
  получена:  $actual
Скачанный файл повреждён или подменён."
    fi

    rm -rf "$gnupg_home"
    info "Подпись верна, контрольная сумма совпала."
}

TMP_DIR=""
cleanup() {
    if [ -n "$TMP_DIR" ]; then
        rm -rf "$TMP_DIR"
    fi
}
trap cleanup EXIT

# --- 1. Preflight ------------------------------------------------------------
info "Проверяем систему перед установкой..."

if [ "$(id -u)" -ne 0 ]; then
    die "Скрипт нужно запускать от root: sudo ./install.sh"
fi

ARCH=$(dpkg --print-architecture)
case "$ARCH" in
    arm64|amd64) ;;
    *) die "Архитектура «$ARCH» не поддерживается. Нужна arm64 или amd64." ;;
esac

# shellcheck disable=SC1091
. /etc/os-release

OS_ID=""
if [ "${ID:-}" = "debian" ]; then
    OS_ID="debian"
elif printf '%s\n' "${ID_LIKE:-}" | tr ' ' '\n' | grep -qx "debian"; then
    # Debian derivative (some Armbian builds): use the Debian repo of Angie.
    OS_ID="debian"
fi
if [ -z "$OS_ID" ]; then
    die "Поддерживаются только Debian 12/13 и производные (Armbian).
Обнаружено: ID=«${ID:-?}», ID_LIKE=«${ID_LIKE:-}». Установка прервана."
fi

case "${VERSION_ID:-}" in
    12|13) ;;
    *) die "Нужен Debian 12 (bookworm) или 13 (trixie), обнаружено: «${VERSION_ID:-неизвестно}».
Debian 11 и старше не поддерживаются: там polkit 0.105 без JS-правил,
на которых построена модель привилегий панели." ;;
esac

CODENAME="${VERSION_CODENAME:-}"
if [ -z "$CODENAME" ]; then
    case "$VERSION_ID" in
        12) CODENAME=bookworm ;;
        13) CODENAME=trixie ;;
    esac
fi

# No RTC on most SBCs: with a skewed clock ACME issuance and TLS fail in
# confusing ways, so warn loudly.
if command -v timedatectl >/dev/null 2>&1; then
    NTP_SYNC=$(timedatectl show -p NTPSynchronized --value 2>/dev/null || true)
    if [ "$NTP_SYNC" != "yes" ]; then
        warn "Системные часы НЕ синхронизированы по NTP (NTPSynchronized=${NTP_SYNC:-нет данных})."
        warn "У многих одноплатников нет RTC: со сбитыми часами выпуск ACME-сертификатов и TLS ломаются."
        warn "Рекомендуется включить синхронизацию: timedatectl set-ntp true"
    fi
else
    warn "timedatectl не найден — не могу проверить синхронизацию времени."
fi

# Ports 80/443 must be free for Angie — unless Angie itself already holds them.
if dpkg -s angie >/dev/null 2>&1; then
    info "Angie уже установлен — проверку портов 80/443 пропускаем."
else
    for port in 80 443; do
        LISTENERS=$(port_listeners "$port")
        if [ -n "$LISTENERS" ]; then
            warn "Порт $port уже занят другим сервисом:"
            printf '%s\n' "$LISTENERS"
            die "Angie не сможет слушать порт $port. Остановите и отключите занимающий его сервис
(например: systemctl disable --now nginx) и запустите установку заново."
        fi
    done
fi

# Panel port and the local status API port: warn but do not abort.
for port in "$DEFAULT_PANEL_PORT" "$STATUS_PORT"; do
    LISTENERS=$(port_listeners "$port")
    if [ -n "$LISTENERS" ]; then
        warn "Порт $port занят (он нужен angie-panel: веб-интерфейс / status-API Angie):"
        printf '%s\n' "$LISTENERS"
        warn "Установка продолжится, но панель может не запуститься, пока порт не освободится."
        warn "Порт панели можно изменить в $PANEL_CONF после установки."
    fi
done

# --- 2. Angie repository and package -----------------------------------------
# gnupg is needed to verify the panel package's signature below — not optional,
# so it goes in with the rest rather than being probed for later.
info "Устанавливаем базовые зависимости (ca-certificates, curl, gnupg)..."
apt-get update
apt-get install -y ca-certificates curl gnupg

info "Скачиваем ключ подписи репозитория Angie..."
# -fsSL so an HTTP error/redirect fails loudly instead of writing an error page
# into the keyring. Scoped to /etc/apt/keyrings + signed-by (below) so the key
# is trusted only for the Angie repo, not every apt source on the system.
install -d -m 0755 /etc/apt/keyrings
if ! curl -fsSL https://angie.software/keys/angie-signing.gpg \
        -o /etc/apt/keyrings/angie-signing.gpg; then
    die "Не удалось скачать ключ подписи Angie (https://angie.software/keys/angie-signing.gpg).
Проверьте доступ в интернет с этого устройства."
fi

REPO_BASE="https://download.angie.software/angie/${OS_ID}/${VERSION_ID}"
info "Проверяем доступность репозитория Angie: ${REPO_BASE}"
if ! curl -fsI "${REPO_BASE}/dists/${CODENAME}/Release" >/dev/null; then
    die "Репозиторий Angie не отвечает: ${REPO_BASE}/dists/${CODENAME}/Release
Проверьте доступ в интернет с этого устройства. Если сеть в порядке,
сверьте адреса репозиториев с документацией: https://angie.software/"
fi

echo "deb [signed-by=/etc/apt/keyrings/angie-signing.gpg] ${REPO_BASE} ${CODENAME} main" \
    > /etc/apt/sources.list.d/angie.list

info "Устанавливаем Angie..."
apt-get update
apt-get install -y angie
systemctl enable --now angie
info "Angie установлен и запущен."

# --- 3. angie-panel package ---------------------------------------------------
DEB_PATH="${1:-}"
if [ -n "$DEB_PATH" ]; then
    if [ ! -f "$DEB_PATH" ]; then
        die "Файл пакета не найден: $DEB_PATH"
    fi
    DEB_PATH=$(realpath "$DEB_PATH")
    # A file the operator handed us: we have nothing to check it against, and
    # second-guessing a deliberate choice would be theatre. Say so plainly.
    warn "Устанавливается локальный файл: $DEB_PATH"
    warn "Его подпись и контрольная сумма не проверяются — проверьте сами:"
    warn "  docs/installation.md, раздел «Проверка подписи артефактов»"
else
    if [ "$PANEL_VERSION" = "__PANEL_VERSION__" ]; then
        die "Эта копия install.sh не привязана к релизу (версия не подставлена).
Запустите install.sh из релиза (releases/latest/download/install.sh) или
передайте путь к локальному .deb:
  sudo ./install.sh ./angie-panel_<версия>_${ARCH}.deb"
    fi
    PANEL_DEB_URL="${PANEL_DEB_URL_TEMPLATE//\{ARCH\}/$ARCH}"
    TMP_DIR=$(mktemp -d)
    # Let apt's sandbox user (_apt) read the downloaded file.
    chmod 755 "$TMP_DIR"
    # Keep the release filename: SHA256SUMS.txt lists files by that name, and
    # renaming here would make the checksum lookup silently miss.
    DEB_NAME=$(basename "$PANEL_DEB_URL")
    DEB_PATH="$TMP_DIR/$DEB_NAME"
    info "Скачиваем пакет angie-panel: $PANEL_DEB_URL"
    if ! curl -fL -o "$DEB_PATH" "$PANEL_DEB_URL"; then
        die "Не удалось скачать пакет angie-panel.
Скачайте .deb вручную со страницы релизов и передайте путь к нему:
  sudo ./install.sh ./angie-panel_<версия>_${ARCH}.deb"
    fi
    verify_panel_deb "$DEB_PATH" "$DEB_NAME"
fi

info "Устанавливаем angie-panel..."
apt-get install -y "$DEB_PATH"

# --- 4. Packaged default site -------------------------------------------------
if [ -f "$DEFAULT_SITE" ]; then
    echo
    echo "Пакет Angie установил сайт-заглушку: $DEFAULT_SITE"
    echo "Панель управляет default-сайтом сама (страница по умолчанию, ACME),"
    echo "и пакетная заглушка будет ей мешать."
    echo "Файл является dpkg-конффайлом: его удаление сохранится при обновлениях Angie."
    read -r -p "Удалить $DEFAULT_SITE? [y/N] " REPLY_DEFAULT
    case "$REPLY_DEFAULT" in
        [yY]|[yY][eE][sS]|[дД]|[дД][аА])
            rm -f "$DEFAULT_SITE"
            if systemctl is-active --quiet angie; then
                systemctl reload angie
            fi
            info "Заглушка удалена."
            ;;
        *)
            warn "Заглушка оставлена. Удалить позже можно так:"
            warn "  rm $DEFAULT_SITE && systemctl reload angie"
            ;;
    esac
fi

# --- 5. Bind address ----------------------------------------------------------
echo
info "Выбор IP-адреса, на котором будет доступна веб-панель."

WAN_IFACE=$(ip route show default 2>/dev/null \
    | awk '/^default/ { for (i = 1; i < NF; i++) if ($i == "dev") { print $(i + 1); exit } }')
if [ -n "$WAN_IFACE" ]; then
    echo "Интерфейс с маршрутом по умолчанию (обычно смотрит в интернет/WAN): $WAN_IFACE"
fi

# Candidate addresses: lines of "iface address" for global-scope IPv4.
mapfile -t CANDIDATES < <(ip -4 -o addr show scope global \
    | awk '{ split($4, a, "/"); print $2, a[1] }')

echo "Доступные адреса:"
echo "  1) 127.0.0.1 — доступ только с самого устройства (безопасно; извне — через SSH-туннель)"
MENU_MAX=1
for entry in "${CANDIDATES[@]}"; do
    iface=${entry%% *}
    addr=${entry##* }
    note=""
    if [ -n "$WAN_IFACE" ] && [ "$iface" = "$WAN_IFACE" ]; then
        note="   <-- WAN-интерфейс, НЕ РЕКОМЕНДУЕТСЯ"
    fi
    MENU_MAX=$((MENU_MAX + 1))
    echo "  $MENU_MAX) $addr ($iface)$note"
done

read -r -p "Введите номер варианта [1]: " CHOICE
CHOICE=${CHOICE:-1}
if ! [[ "$CHOICE" =~ ^[0-9]+$ ]] || [ "$CHOICE" -lt 1 ] || [ "$CHOICE" -gt "$MENU_MAX" ]; then
    die "Некорректный выбор: «$CHOICE». Запустите скрипт заново."
fi

if [ "$CHOICE" -eq 1 ]; then
    BIND_IFACE="lo"
    BIND_ADDR="127.0.0.1"
else
    entry="${CANDIDATES[$((CHOICE - 2))]}"
    BIND_IFACE=${entry%% *}
    BIND_ADDR=${entry##* }
fi

# Refuse to expose the panel on the default-route (WAN) interface unless the
# user types an explicit confirmation phrase.
if [ -n "$WAN_IFACE" ] && [ "$BIND_IFACE" = "$WAN_IFACE" ]; then
    echo
    warn "Адрес $BIND_ADDR принадлежит WAN-интерфейсу ($WAN_IFACE)."
    warn "Панель станет доступна ИЗ ИНТЕРНЕТА — это серьёзный риск взлома устройства."
    warn "Правильный путь: выбрать LAN-адрес, а наружу публиковать панель через сам Angie"
    warn "(proxy-хост с сертификатом и ограничением по IP)."
    read -r -p "Чтобы всё равно продолжить, введите фразу «${CONFIRM_PHRASE}»: " TYPED
    if [ "$TYPED" != "$CONFIRM_PHRASE" ]; then
        die "Подтверждение не получено — установка прервана.
Запустите скрипт заново и выберите LAN-адрес."
    fi
fi

if [ ! -f "$PANEL_CONF" ]; then
    die "Не найден $PANEL_CONF — похоже, пакет angie-panel установился некорректно."
fi
info "Записываем bind_addr = \"$BIND_ADDR\" в $PANEL_CONF"
sed -i "s|^bind_addr = .*|bind_addr = \"${BIND_ADDR}\"|" "$PANEL_CONF"

# --- 6. Start the panel and print the setup token ------------------------------
info "Запускаем angie-panel..."
systemctl enable --now angie-panel
# The package may have started the unit before we changed bind_addr —
# restart so the new address takes effect.
systemctl restart angie-panel

PANEL_PORT=$(sed -n 's/^port[[:space:]]*=[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$PANEL_CONF" | head -n 1)
PANEL_PORT=${PANEL_PORT:-$DEFAULT_PANEL_PORT}

info "Ожидаем создание setup-токена (до 15 секунд)..."
TOKEN=""
for _ in {1..15}; do
    if [ -s "$TOKEN_FILE" ]; then
        TOKEN=$(cat "$TOKEN_FILE")
        break
    fi
    sleep 1
done

if [ -z "$TOKEN" ]; then
    die "Setup-токен не появился за 15 секунд — панель, вероятно, не стартовала.
Диагностика: journalctl -u angie-panel -e
После устранения проблемы новый токен можно получить командой:
  sudo angie-panel reset-password"
fi

echo
echo "================================================================"
info "Установка завершена!"
echo
echo "  Откройте в браузере:  http://${BIND_ADDR}:${PANEL_PORT}/setup"
echo "  Одноразовый токен:    ${TOKEN}"
echo
echo "  Токен одноразовый и действует ограниченное время."
echo "  Если токен истёк или пароль утерян, выполните:"
echo "      sudo angie-panel reset-password"
echo "  — команда напечатает новый setup-токен, данные панели не пострадают."
echo
echo "  Вместе с панелью установлен apctl — CLI к тому же API:"
echo "      sudo apctl status      # состояние панели и Angie"
echo "      sudo apctl apply       # применить конфигурацию"
echo "      sudo apctl export      # выгрузить конфигурацию в JSON"
echo "  На этом сервере токен ему не нужен. Подробности: man apctl"
echo "================================================================"
