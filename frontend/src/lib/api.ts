export interface AuthState {
  setup_required: boolean
  authenticated: boolean
}

export type Role = 'admin' | 'viewer'

export interface Me {
  email: string
  role: Role
}

export interface User {
  id: number
  email: string
  role: Role
  /** Unix timestamp, seconds. */
  created_at: number
}

export interface OkResponse {
  ok: true
}

export interface SetupRequest {
  token: string
  email: string
  password: string
}

export interface LoginRequest {
  email: string
  password: string
}

export interface SystemStatus {
  panel: {
    version: string
    data_dir_writable: boolean
  }
  angie: {
    installed: boolean
    version: string | null
    acme_module: boolean | null
    unit_active: boolean | null
  }
  dbus: {
    available: boolean
    polkit_ok: boolean | null
  }
  status_api: {
    reachable: boolean
    generation: number | null
  }
}

export interface ConfigtestReport {
  /** Unix timestamp, seconds. */
  timestamp: number
  ok: boolean
  exit_code: number | null
  output: string
  ran_via: 'systemd' | 'direct'
}

export type ForwardScheme = 'http' | 'https'

export interface Location {
  path: string
  forward_scheme: ForwardScheme
  forward_host: string
  forward_port: number
  rewrite?: string | null
  snippet?: string | null
}

/** Per-host rate limiting (Angie limit_req / limit_conn, keyed on client IP). */
export interface RateLimit {
  enabled: boolean
  /** Requests/second ceiling (0 = no request-rate limit). */
  rps: number
  /** Burst allowance above the rate. */
  burst: number
  /** Serve the burst immediately instead of queueing it. */
  nodelay: boolean
  /** Max concurrent connections per client IP (0 = no limit). */
  conn: number
}

export type BalanceMethod = 'round_robin' | 'least_conn' | 'ip_hash'

/** One additional backend server beyond the primary (forward_host:port). */
export interface UpstreamServer {
  host: string
  port: number
  weight: number
  backup: boolean
  down: boolean
}

/** Load balancing + passive health for a host's upstream pool. */
export interface Upstream {
  /** Additional peers beyond the primary. */
  servers: UpstreamServer[]
  method: BalanceMethod
  /** Weight of the primary server. */
  primary_weight: number
  /** Passive health: mark a peer failed after N errors within fail_timeout. */
  max_fails: number
  fail_timeout_secs: number
}

export interface Ban {
  id: number
  /** Bare IP or IP/CIDR (v4 or v6). */
  address: string
  reason: string | null
  /** Unix timestamp, seconds. */
  created_at: number
}

/** One audited mutation (who did what, and the outcome). */
export interface AuditEntry {
  id: number
  /** null for unauthenticated requests (e.g. a login attempt). */
  user_email: string | null
  method: string
  path: string
  status: number
  /** Unix timestamp, seconds. */
  created_at: number
}

/** Global country access mode. */
export type GeoMode = 'off' | 'deny' | 'allow'

/** Global country policy: mode + ISO 3166-1 alpha-2 country codes. */
export interface GeoPolicy {
  mode: GeoMode
  countries: string[]
}

/** Mutual TLS: require/verify client certificates against a CA bundle. */
export interface Mtls {
  /** CA bundle (PEM) that verifies presented client certs. null = mTLS off. */
  ca_pem: string | null
  /** Request a cert but don't reject clients that omit one (pass result upstream). */
  optional: boolean
}

/** Where a custom header is applied. */
export type HeaderDirection = 'request' | 'response'

/** A user-defined header added to responses (add_header) or requests (proxy_set_header). */
export interface CustomHeader {
  name: string
  value: string
  direction: HeaderDirection
}

/** Per-host maintenance mode: serve a styled 503 page instead of proxying. */
export interface Maintenance {
  enabled: boolean
  title: string
  message: string
}

/** Per-host gzip response compression. */
export interface Gzip {
  enabled: boolean
  /** 1-9; 0 = omit (Angie default). */
  comp_level: number
  /** Minimum bytes to compress; 0 = omit. */
  min_length: number
  /** Extra MIME types to compress; empty = a curated default set. */
  types: string[]
}

/** One custom error page: a plain-text title + message rendered into a template. */
export interface ErrorPage {
  enabled: boolean
  title: string
  message: string
}

/** Per-host custom error pages. `not_found` covers upstream 404s (status kept);
 *  `server_error` covers 500/502/503/504 (all served as 503). */
export interface ErrorPages {
  not_found: ErrorPage
  server_error: ErrorPage
}

/** Forward authentication (SSO gateway) via Angie's auth_request. */
export interface ForwardAuth {
  enabled: boolean
  /** Internal verification endpoint (the SSO service's auth check). */
  verify_url: string
  /** Optional 401 redirect target — the SSO sign-in page. null = return 401. */
  sign_in_url: string | null
  /** Identity headers from the auth response to forward to the upstream. */
  copy_headers: string[]
}

export interface Host {
  id: number
  domains: string[]
  forward_scheme: ForwardScheme
  forward_host: string
  forward_port: number
  websockets_upgrade: boolean
  block_exploits: boolean
  cache_assets: boolean
  http2: boolean
  http3: boolean
  force_ssl: boolean
  hsts: boolean
  hsts_subdomains: boolean
  trust_forwarded_proto: boolean
  certificate_id: number | null
  access_list_id: number | null
  locations: Location[]
  advanced_snippet: string | null
  rate_limit: RateLimit
  upstream: Upstream
  mtls: Mtls
  forward_auth: ForwardAuth
  custom_headers: CustomHeader[]
  maintenance: Maintenance
  gzip: Gzip
  error_pages: ErrorPages
  enabled: boolean
  /** Unix timestamp, seconds. */
  created_at: number
  /** Unix timestamp, seconds. */
  updated_at: number
}

/** Host without id/created_at/updated_at — the create/update payload. */
export interface HostInput {
  domains: string[]
  forward_scheme: ForwardScheme
  forward_host: string
  forward_port: number
  websockets_upgrade?: boolean
  block_exploits?: boolean
  cache_assets?: boolean
  http2?: boolean
  http3?: boolean
  force_ssl?: boolean
  hsts?: boolean
  hsts_subdomains?: boolean
  trust_forwarded_proto?: boolean
  certificate_id?: number | null
  access_list_id?: number | null
  locations?: Location[]
  advanced_snippet?: string | null
  rate_limit?: RateLimit
  upstream?: Upstream
  mtls?: Mtls
  forward_auth?: ForwardAuth
  custom_headers?: CustomHeader[]
  maintenance?: Maintenance
  gzip?: Gzip
  error_pages?: ErrorPages
  enabled?: boolean
}

/** Redirection hosts may keep the incoming scheme ("auto") or force one. */
export type RedirectForwardScheme = 'auto' | 'http' | 'https'

export interface RedirectHost {
  id: number
  domains: string[]
  forward_scheme: RedirectForwardScheme
  forward_domain: string
  forward_http_code: number
  preserve_path: boolean
  certificate_id: number | null
  force_ssl: boolean
  hsts: boolean
  hsts_subdomains: boolean
  http2: boolean
  block_exploits: boolean
  advanced_snippet: string | null
  enabled: boolean
  /** Unix timestamp, seconds. */
  created_at: number
  /** Unix timestamp, seconds. */
  updated_at: number
}

/** A TCP/UDP port forward (Angie stream {} context). */
/** TLS handling on the incoming port. */
export type StreamTls = 'none' | 'terminate'

export interface Stream {
  id: number
  incoming_port: number
  forward_host: string
  forward_port: number
  tcp: boolean
  udp: boolean
  tls: StreamTls
  /** Certificate used when tls === 'terminate'. */
  certificate_id: number | null
  enabled: boolean
  /** Unix timestamp, seconds. */
  created_at: number
  /** Unix timestamp, seconds. */
  updated_at: number
}

/** Stream without id/created_at/updated_at — the create/update payload. */
export interface StreamInput {
  incoming_port: number
  forward_host: string
  forward_port: number
  tcp?: boolean
  udp?: boolean
  tls?: StreamTls
  certificate_id?: number | null
  enabled?: boolean
}

/** One SNI → backend route inside an SNI router. */
export interface SniRoute {
  /** Exact hostname or `*.`-prefixed wildcard. */
  sni: string
  forward_host: string
  forward_port: number
}

/** An SNI passthrough router: one stream listener that forwards TLS connections
 *  by SNI hostname without terminating TLS (ssl_preread). */
export interface SniRouter {
  id: number
  name: string
  incoming_port: number
  routes: SniRoute[]
  /** Catch-all backend for unmatched/absent SNI ('' / 0 = none, drop). */
  default_host: string
  default_port: number
  enabled: boolean
  /** Unix timestamp, seconds. */
  created_at: number
  /** Unix timestamp, seconds. */
  updated_at: number
}

/** SniRouter without id/created_at/updated_at — the create/update payload. */
export interface SniRouterInput {
  name: string
  incoming_port: number
  routes: SniRoute[]
  default_host?: string
  default_port?: number
  enabled?: boolean
}

/** RedirectHost without id/created_at/updated_at — the create/update payload. */
export interface RedirectHostInput {
  domains: string[]
  forward_scheme?: RedirectForwardScheme
  forward_domain: string
  forward_http_code?: number
  preserve_path?: boolean
  certificate_id?: number | null
  force_ssl?: boolean
  hsts?: boolean
  hsts_subdomains?: boolean
  http2?: boolean
  block_exploits?: boolean
  advanced_snippet?: string | null
  enabled?: boolean
}

export interface DeadHost {
  id: number
  domains: string[]
  certificate_id: number | null
  force_ssl: boolean
  hsts: boolean
  hsts_subdomains: boolean
  http2: boolean
  advanced_snippet: string | null
  enabled: boolean
  /** Unix timestamp, seconds. */
  created_at: number
  /** Unix timestamp, seconds. */
  updated_at: number
}

/** DeadHost without id/created_at/updated_at — the create/update payload. */
export interface DeadHostInput {
  domains: string[]
  certificate_id?: number | null
  force_ssl?: boolean
  hsts?: boolean
  hsts_subdomains?: boolean
  http2?: boolean
  advanced_snippet?: string | null
  enabled?: boolean
}

export type FileStatus = 'added' | 'modified' | 'removed' | 'unchanged'

export interface FileDiff {
  name: string
  status: FileStatus
  unified: string
  drift: boolean
}

export interface DiffReport {
  files: FileDiff[]
  foreign: { name: string }[]
  added: number
  modified: number
  removed: number
  unchanged: number
  has_drift: boolean
}

export type ApplyResult =
  | 'ok'
  | 'lint_failed'
  | 'validation_failed'
  | 'reload_failed'
  | 'error'

export interface LintViolation {
  file: string
  line: number | null
  message: string
}

export interface FileErrorItem {
  file: string | null
  line: number | null
  message: string
}

export interface RollbackInfo {
  attempted: boolean
  ok: boolean
  detail: string
}

export interface ApplyReport {
  /** Unix timestamp, seconds. */
  timestamp: number
  result: ApplyResult
  diff?: DiffReport
  lint_violations: LintViolation[]
  stderr: string
  file_errors: FileErrorItem[]
  error_log_tail: string
  rollback?: RollbackInfo
  synthetic_base: boolean
  summary: string
}

export interface ApplyPreview {
  db_revision: number
  diff: DiffReport
}

export interface ApplyHistoryEntry {
  id: number
  /** Unix timestamp, seconds. */
  timestamp: number
  result: string
  report: ApplyReport
}

export type DefaultSite = 'notfound' | 'drop444' | 'redirect' | 'html'

export interface EffectiveSettings {
  default_site: string
  ipv6_enabled: boolean
  resolvers: string[]
}

export interface SettingsResponse {
  raw: Record<string, string>
  effective: EffectiveSettings
}

export type AcmeChallenge = 'http' | 'dns' | 'alpn'

/** One credential field a DNS provider type needs (maps to an acme.sh env var). */
export interface DnsProviderField {
  env: string
  label: string
}

/** A DNS-01 provider TYPE from the registry (acme.sh dnsapi under the hood). */
export interface DnsProviderInfo {
  id: string
  label: string
  fields: DnsProviderField[]
}

/** A named credential profile — one account of a provider type. Several of the
 *  same type may coexist. A certificate references a profile by its id. */
export interface DnsCredentialProfile {
  id: number
  /** Provider type id (e.g. "cloudflare"). */
  provider: string
  provider_label: string
  name: string
  configured: boolean
}

export type AcmeKeyType = 'ecdsa' | 'rsa'

/**
 * A snapshot of Angie's ACME client status
 * (/status/http/acme_clients/<name>). Every field is optional: the status API
 * may be unreachable (the whole object is null) or report only part of this
 * shape, so consumers must treat missing fields as unknown.
 */
export interface AcmeStatus {
  state?: string
  /** e.g. "valid" | "expired" | "missing" | … */
  certificate?: string
  details?: string
  next_run?: string | number
}

export interface Cert {
  id: number
  name: string
  domains: string[]
  challenge: AcmeChallenge
  key_type: AcmeKeyType
  email: string | null
  staging: boolean
  /** For a DNS-01 cert: provider id that fulfils the challenge (null = Angie
   *  answers DNS itself via NS delegation). */
  dns_provider: string | null
  /** Unix timestamp, seconds. */
  created_at: number
  status: AcmeStatus | null
}

/** Cert without id/created_at/status — the create payload. */
export interface CertInput {
  name: string
  domains: string[]
  challenge?: AcmeChallenge
  key_type?: AcmeKeyType
  email?: string | null
  staging?: boolean
  dns_provider?: string | null
}

export interface DelegationHint {
  domain: string
  requires: string
  records: string[]
}

export interface CertPrecheck {
  challenge: string
  resolvers: string[]
  delegation_hints: DelegationHint[]
}

export type AccessListSatisfy = 'any' | 'all'

export type AccessListDirective = 'allow' | 'deny'

export interface AccessListUser {
  username: string
  /** The password hash is never exposed by the API. */
  has_password: boolean
}

export interface AccessListClient {
  directive: AccessListDirective
  address: string
}

export interface AccessList {
  id: number
  name: string
  satisfy: AccessListSatisfy
  pass_auth: boolean
  users: AccessListUser[]
  clients: AccessListClient[]
  /** Unix timestamp, seconds. */
  created_at: number
}

export interface AccessListUserInput {
  username: string
  /** Omitted on update to keep the existing password unchanged. */
  password?: string
}

/** AccessList without id/created_at — the create/update payload. */
export interface AccessListInput {
  name: string
  satisfy?: AccessListSatisfy
  pass_auth?: boolean
  users: AccessListUserInput[]
  clients: AccessListClient[]
}

// --- Live dashboard (M3) ---------------------------------------------------

export interface DashboardConnections {
  accepted: number
  active: number
  idle: number
  dropped: number
}

export interface DashboardAngie {
  up: boolean
  version: string | null
  generation: number | null
  load_time: string | null
  connections: DashboardConnections | null
}

export interface DashboardRequests {
  total: number
  processing: number
  discarded: number
}

export interface DashboardZone {
  requests: DashboardRequests
  /** Keyed by HTTP status code (or "Nxx" bucket); any non-status key is ignored. */
  responses: Record<string, number>
  data: { received: number; sent: number }
}

export interface DashboardUpstream {
  peers_up: number
  peers_down: number
  fails: number
}

export interface DashboardHost {
  id: number
  domains: string[]
  enabled: boolean
  forward: string
  certificate_id: number | null
  https_active: boolean
  /** null when the host has no traffic yet or Angie's status API is down. */
  zone: DashboardZone | null
  upstream: DashboardUpstream | null
}

export interface DashboardCert {
  id: number
  name: string
  domains: string[]
  challenge: AcmeChallenge
  staging: boolean
  status: AcmeStatus | null
}

export interface DashboardDrift {
  detected: boolean
  foreign_files: string[]
}

export type AlertSeverity = 'error' | 'warning' | 'info'

export interface DashboardAlert {
  severity: AlertSeverity
  /** Stable machine code, e.g. "angie_down" | "cert_failed" | "drift" | "pending". */
  code: string
  message: string
}

export interface DashboardStreams {
  configured: number
  enabled: number
  /** Whether the Angie stream {} context is active (loads stream.d). */
  context_active: boolean
}

export interface Dashboard {
  angie: DashboardAngie
  hosts: DashboardHost[]
  certificates: DashboardCert[]
  streams: DashboardStreams
  drift: DashboardDrift
  pending_changes: boolean
  alerts: DashboardAlert[]
}

export class ApiError extends Error {
  readonly status: number
  readonly code: string

  constructor(status: number, code: string, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.code = code
  }
}

type Method = 'GET' | 'POST' | 'PUT' | 'DELETE'

interface RequestOptions {
  /**
   * When false, a 401 response does not redirect to /login.
   * Used by the auth endpoints themselves.
   */
  redirectOn401?: boolean
}

interface ErrorBody {
  error: {
    code: string
    message: string
  }
}

function isErrorBody(data: unknown): data is ErrorBody {
  if (typeof data !== 'object' || data === null || !('error' in data)) {
    return false
  }
  const error = (data as { error: unknown }).error
  if (typeof error !== 'object' || error === null) {
    return false
  }
  const { code, message } = error as { code?: unknown; message?: unknown }
  return typeof code === 'string' && typeof message === 'string'
}

function redirectToLogin(): void {
  if (window.location.pathname !== '/login') {
    window.location.assign('/login')
  }
}

async function request<T>(
  method: Method,
  path: string,
  body?: unknown,
  options: RequestOptions = {},
): Promise<T> {
  const headers: Record<string, string> = {}
  if (method !== 'GET') {
    // CSRF defense: the backend rejects mutating requests without this header.
    headers['X-AP-Request'] = '1'
  }
  if (body !== undefined) {
    headers['Content-Type'] = 'application/json'
  }

  const response = await fetch(path, {
    method,
    credentials: 'same-origin',
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })

  if (response.ok) {
    return (await response.json()) as T
  }

  let code = 'unknown_error'
  let message = `Request failed with status ${response.status}`
  try {
    const data: unknown = await response.json()
    if (isErrorBody(data)) {
      code = data.error.code
      message = data.error.message
    }
  } catch {
    // Non-JSON error body; keep the fallback code and message.
  }

  if (response.status === 401 && options.redirectOn401 !== false) {
    redirectToLogin()
  }

  throw new ApiError(response.status, code, message)
}

export const api = {
  getAuthState: () =>
    request<AuthState>('GET', '/api/auth/state', undefined, { redirectOn401: false }),

  setup: (body: SetupRequest) =>
    request<OkResponse>('POST', '/api/auth/setup', body, { redirectOn401: false }),

  login: (body: LoginRequest) =>
    request<OkResponse>('POST', '/api/auth/login', body, { redirectOn401: false }),

  logout: () => request<OkResponse>('POST', '/api/auth/logout', undefined, { redirectOn401: false }),

  me: () => request<Me>('GET', '/api/auth/me'),

  listUsers: () => request<{ users: User[] }>('GET', '/api/users'),

  createUser: (body: { email: string; password: string; role: Role }) =>
    request<User>('POST', '/api/users', body),

  setUserRole: (id: number, role: Role) =>
    request<User>('PUT', `/api/users/${id}/role`, { role }),

  deleteUser: (id: number) => request<OkResponse>('DELETE', `/api/users/${id}`),

  changeOwnPassword: (body: {
    current_password: string
    new_password: string
  }) => request<OkResponse>('POST', '/api/users/me/password', body),

  getDashboard: () => request<Dashboard>('GET', '/api/dashboard'),

  getSystemStatus: () => request<SystemStatus>('GET', '/api/system/status'),

  runConfigtest: () => request<ConfigtestReport>('POST', '/api/system/configtest'),

  getLastConfigtest: () => request<ConfigtestReport>('GET', '/api/system/configtest'),

  listHosts: () => request<{ hosts: Host[] }>('GET', '/api/hosts'),

  createHost: (body: HostInput) => request<Host>('POST', '/api/hosts', body),

  getHost: (id: number) => request<Host>('GET', `/api/hosts/${id}`),

  updateHost: (id: number, body: HostInput) =>
    request<Host>('PUT', `/api/hosts/${id}`, body),

  deleteHost: (id: number) => request<OkResponse>('DELETE', `/api/hosts/${id}`),

  enableHost: (id: number) =>
    request<{ ok: true; enabled: true }>('POST', `/api/hosts/${id}/enable`),

  disableHost: (id: number) =>
    request<{ ok: true; enabled: false }>('POST', `/api/hosts/${id}/disable`),

  listRedirectHosts: () =>
    request<{ redirect_hosts: RedirectHost[] }>('GET', '/api/redirect-hosts'),

  createRedirectHost: (body: RedirectHostInput) =>
    request<RedirectHost>('POST', '/api/redirect-hosts', body),

  getRedirectHost: (id: number) =>
    request<RedirectHost>('GET', `/api/redirect-hosts/${id}`),

  updateRedirectHost: (id: number, body: RedirectHostInput) =>
    request<RedirectHost>('PUT', `/api/redirect-hosts/${id}`, body),

  deleteRedirectHost: (id: number) =>
    request<OkResponse>('DELETE', `/api/redirect-hosts/${id}`),

  enableRedirectHost: (id: number) =>
    request<{ ok: true; enabled: true }>('POST', `/api/redirect-hosts/${id}/enable`),

  disableRedirectHost: (id: number) =>
    request<{ ok: true; enabled: false }>('POST', `/api/redirect-hosts/${id}/disable`),

  listDeadHosts: () =>
    request<{ dead_hosts: DeadHost[] }>('GET', '/api/dead-hosts'),

  createDeadHost: (body: DeadHostInput) =>
    request<DeadHost>('POST', '/api/dead-hosts', body),

  getDeadHost: (id: number) => request<DeadHost>('GET', `/api/dead-hosts/${id}`),

  updateDeadHost: (id: number, body: DeadHostInput) =>
    request<DeadHost>('PUT', `/api/dead-hosts/${id}`, body),

  deleteDeadHost: (id: number) =>
    request<OkResponse>('DELETE', `/api/dead-hosts/${id}`),

  enableDeadHost: (id: number) =>
    request<{ ok: true; enabled: true }>('POST', `/api/dead-hosts/${id}/enable`),

  disableDeadHost: (id: number) =>
    request<{ ok: true; enabled: false }>('POST', `/api/dead-hosts/${id}/disable`),

  listBans: () => request<{ bans: Ban[] }>('GET', '/api/bans'),

  createBan: (body: { address: string; reason?: string | null }) =>
    request<Ban>('POST', '/api/bans', body),

  deleteBan: (id: number) => request<OkResponse>('DELETE', `/api/bans/${id}`),

  getGeo: () => request<GeoPolicy>('GET', '/api/geo'),

  putGeo: (body: GeoPolicy) => request<GeoPolicy>('PUT', '/api/geo', body),

  listAudit: () => request<{ entries: AuditEntry[] }>('GET', '/api/audit'),

  listStreams: () => request<{ streams: Stream[] }>('GET', '/api/streams'),

  createStream: (body: StreamInput) =>
    request<Stream>('POST', '/api/streams', body),

  getStream: (id: number) => request<Stream>('GET', `/api/streams/${id}`),

  updateStream: (id: number, body: StreamInput) =>
    request<Stream>('PUT', `/api/streams/${id}`, body),

  deleteStream: (id: number) =>
    request<OkResponse>('DELETE', `/api/streams/${id}`),

  enableStream: (id: number) =>
    request<{ ok: true; enabled: true }>('POST', `/api/streams/${id}/enable`),

  disableStream: (id: number) =>
    request<{ ok: true; enabled: false }>('POST', `/api/streams/${id}/disable`),

  /** One-time activation of the Angie stream {} context (privileged). */
  enableStreamContext: () =>
    request<{ ok: boolean; already_active?: boolean; message?: string }>(
      'POST',
      '/api/streams/enable-context',
    ),

  listSniRouters: () =>
    request<{ sni_routers: SniRouter[] }>('GET', '/api/sni-routers'),

  createSniRouter: (body: SniRouterInput) =>
    request<SniRouter>('POST', '/api/sni-routers', body),

  updateSniRouter: (id: number, body: SniRouterInput) =>
    request<SniRouter>('PUT', `/api/sni-routers/${id}`, body),

  deleteSniRouter: (id: number) =>
    request<OkResponse>('DELETE', `/api/sni-routers/${id}`),

  enableSniRouter: (id: number) =>
    request<{ ok: true; enabled: true }>(
      'POST',
      `/api/sni-routers/${id}/enable`,
    ),

  disableSniRouter: (id: number) =>
    request<{ ok: true; enabled: false }>(
      'POST',
      `/api/sni-routers/${id}/disable`,
    ),

  getApplyPreview: () => request<ApplyPreview>('GET', '/api/apply/preview'),

  apply: () => request<ApplyReport>('POST', '/api/apply'),

  getApplyHistory: () =>
    request<{ history: ApplyHistoryEntry[] }>('GET', '/api/apply/history'),

  getSettings: () => request<SettingsResponse>('GET', '/api/settings'),

  updateSettings: (body: Record<string, string>) =>
    request<SettingsResponse>('PUT', '/api/settings', body),

  /** The static registry of provider TYPES (for the "add profile" form). */
  listDnsProviders: () =>
    request<{ providers: DnsProviderInfo[] }>('GET', '/api/dns-providers'),

  /** The operator's credential profiles (several per type possible). */
  listDnsCredentials: () =>
    request<{ credentials: DnsCredentialProfile[] }>('GET', '/api/dns-credentials'),

  createDnsCredential: (body: {
    provider: string
    name: string
    credentials: Record<string, string>
  }) => request<DnsCredentialProfile>('POST', '/api/dns-credentials', body),

  updateDnsCredential: (
    id: number,
    body: { name?: string; credentials?: Record<string, string> },
  ) => request<DnsCredentialProfile>('PUT', `/api/dns-credentials/${id}`, body),

  deleteDnsCredential: (id: number) =>
    request<OkResponse>('DELETE', `/api/dns-credentials/${id}`),

  listCertificates: () =>
    request<{ certificates: Cert[] }>('GET', '/api/certificates'),

  getCertificate: (id: number) =>
    request<Cert>('GET', `/api/certificates/${id}`),

  createCertificate: (body: CertInput) =>
    request<Cert>('POST', '/api/certificates', body),

  deleteCertificate: (id: number) =>
    request<OkResponse>('DELETE', `/api/certificates/${id}`),

  precheckCertificate: (id: number) =>
    request<CertPrecheck>('POST', `/api/certificates/${id}/precheck`),

  listAccessLists: () =>
    request<{ access_lists: AccessList[] }>('GET', '/api/access-lists'),

  getAccessList: (id: number) =>
    request<AccessList>('GET', `/api/access-lists/${id}`),

  createAccessList: (body: AccessListInput) =>
    request<AccessList>('POST', '/api/access-lists', body),

  updateAccessList: (id: number, body: AccessListInput) =>
    request<AccessList>('PUT', `/api/access-lists/${id}`, body),

  deleteAccessList: (id: number) =>
    request<OkResponse>('DELETE', `/api/access-lists/${id}`),

  exportConfig: () => request<unknown>('GET', '/api/export'),

  importConfig: (doc: unknown) =>
    request<ImportResult>('POST', '/api/import', doc),
}

export interface ImportResult {
  ok: boolean
  imported: {
    certificates: number
    access_lists: number
    hosts: number
    redirect_hosts: number
    dead_hosts: number
    streams: number
    bans: number
    settings: number
  }
}
