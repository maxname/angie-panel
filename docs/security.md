# Модель безопасности / Security model

> Честная модель угроз. Прочтите перед тем, как открывать панель кому-либо.
> Honest threat model — read before exposing the panel.

## Русский

### Главное: панель управляет root-сервисом

Angie master работает от root. Панель генерирует его конфигурацию. **Кто контролирует
конфигурацию Angie — контролирует хост** (через директивы вроде `error_log`, `root`,
`proxy_pass unix:` root-мастер исполняет от root). Поэтому:

- **Компрометация панели ≈ компрометация хоста.** Непривилегированность процесса панели
  защищает от её *багов*, а не от её *захвата*. Относитесь к доступу к панели как к
  root-доступу.

### Чем ограничены привилегии

- Процесс панели работает от пользователя `angie-panel` **без прав на `/etc`** и **без sudo**.
  Юнит захардене (`NoNewPrivileges`, `ProtectSystem=strict`, `CapabilityBoundingSet=`,
  `SystemCallFilter=@system-service`, …).
- Всё, что пишет в `/etc/angie/http.d`, делает маленький **root-хелпер** — oneshot-юнит с
  фиксированным `ExecStart` (панель не контролирует аргументы), запускаемый через D-Bus +
  polkit-правило ровно на два юнита. Это единственная граница доверия с правами.
- Перед записью хелпер прогоняет **линтер-allowlist** по сгенерированным файлам (запрещает
  `load_module`, произвольные `error_log`/`root`, `proxy_pass` на unix-сокеты и
  management-порты, скриптовые модули и т.п.) и валидирует через `angie -t` на staging-копии
  **до** касания живого конфига.

### Валидация ввода = защита от инъекций

`angie -t` НЕ защищает от инъекций: синтаксически валидная внедрённая директива и есть атака.
Поэтому каждое пользовательское поле, попадающее в конфиг, проходит **allowlist-and-reject**:
домены — через idna + строгий шаблон; `forward_host` — только голый IP или hostname (никаких
`;{}` / путей / портов внутри); пути локаций — префиксные; и т.д. То же самое применяется к
**импорту конфигурации** — импортируемый JSON не доверенный.

### Сниппеты = root-эквивалент

Поле «advanced snippet» вставляется в конфиг, исполняемый root. Поэтому оно **выключено по
умолчанию** и включается только root-owned флагом `/etc/angie-panel.toml:
allow_advanced_snippets = true` (веб-сессия не может его переставить). Даже включённые
сниппеты проходят линтер.

### Что сделано против типичных дыр (уроки NPM CVE)

- **Нет дефолтных паролей** — одноразовый setup-токен (256 бит, TTL 24ч, файл 0600,
  constant-time сравнение).
- **CSRF** — обязательный заголовок `X-AP-Request` + проверка `Origin`; cookie HttpOnly +
  SameSite=Lax; **CORS не выдаётся вовсе** (урок CVE-2025-50579).
- **DNS-rebinding** — allowlist `Host`-заголовка.
- **SSRF** — `forward_host` на loopback/link-local запрещён без явного opt-in (иначе можно
  опубликовать неаутентифицированный `/status` API).
- **CSP** `default-src 'self'`, `frame-ancestors 'none'`; весь вывод рендерится как
  экранированный текст (нет `dangerouslySetInnerHTML`).
- Аргон2id для паролей; rate-limit на логин; внешние команды — только argv, без shell.

### Резервные копии содержат секреты

Экспорт конфигурации и снапшоты бэкапов могут содержать секреты (например, в сниппетах —
inline `Authorization`-заголовки). Каталог `/var/lib/angie-panel` — 0700, БД — 0600. Храните
экспортированные JSON надёжно.

### DNS-01 и UDP/53

Актуально **только для режима «Angie отвечает сам» (NS-делегирование)**: тогда Angie отвечает
на валидационные DNS-запросы на UDP/53. Не открывайте UDP/53 в интернет шире, чем нужно для
валидации (амплификация/сканирование); Angie отвечает только на `_acme-challenge`-запросы
делегированной зоны. В режиме **API DNS-провайдера** (по умолчанию) входящий UDP/53 не нужен —
панель создаёт TXT-запись через API провайдера, а доступы к провайдерам хранятся как секреты
только-на-запись (`dns_cred:*`, не отдаются в GET и не попадают в бэкапы).

---

## English (summary)

- **The panel configures a root service (Angie).** Whoever controls that config controls the
  host. The panel's unprivileged process protects against its *bugs*, not against its
  *takeover* — treat panel access as root access.
- Privilege containment: the panel runs as `angie-panel` with no `/etc` access and no sudo
  (hardened unit); all writes to `/etc/angie/http.d` go through a small fixed-argv **root
  helper** (oneshot units, D-Bus + a polkit rule scoped to exactly those two units), which
  runs a **directive allowlist linter** over the generated files and `angie -t` on a staging
  copy before touching the live config.
- **Input validation is the injection defense** (`angie -t` is not): every user field is
  allowlist-validated and rejected, never escaped — including on config **import** (untrusted).
- **Advanced snippets are root-equivalent** — disabled unless a root-owned config flag enables
  them, and still linted.
- Learned from NPM CVEs: no default credentials (one-time setup token), CSRF header + Origin
  check, no CORS ever, Host allowlist (DNS-rebinding), SSRF guard on upstreams, CSP, argon2id,
  login rate-limit, shell-free argv commands.
- **Roles** (admin / viewer): authorization is enforced at one central choke point
  (`security_layer`) — every mutating request from a non-admin is rejected, with a small
  self-service allowlist (login/setup/logout + change own password). A viewer, or any future
  endpoint, can never mutate config even if a handler forgets to check.
- **IP blocklist**: banned IPs/CIDRs are generated as http-scope `deny` rules (03-bans.conf) and
  return 403. This is the panel-native enforcement point fail2ban / CrowdSec can drive (push a ban
  via the API, then apply). Caveat: the global `deny` is NOT inherited by hosts that define their
  own IP access rules (access lists) — add the IP to those lists too if needed.
- The config export is a full backup (every host type, certificates, access lists, settings) and
  contains secrets — advanced-snippet contents and basic-auth password hashes — so treat the file
  as sensitive. On import those hashes are shape-checked as bcrypt (they land in an htpasswd file).
  `/var/lib/angie-panel` is 0700, the DB 0600.
