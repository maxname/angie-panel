export interface AuthState {
  setup_required: boolean
  authenticated: boolean
}

export interface Me {
  email: string
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
  force_ssl: boolean
  hsts: boolean
  hsts_subdomains: boolean
  trust_forwarded_proto: boolean
  certificate_id: number | null
  locations: Location[]
  advanced_snippet: string | null
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
  force_ssl?: boolean
  hsts?: boolean
  hsts_subdomains?: boolean
  trust_forwarded_proto?: boolean
  certificate_id?: number | null
  locations?: Location[]
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

  getApplyPreview: () => request<ApplyPreview>('GET', '/api/apply/preview'),

  apply: () => request<ApplyReport>('POST', '/api/apply'),

  getApplyHistory: () =>
    request<{ history: ApplyHistoryEntry[] }>('GET', '/api/apply/history'),

  getSettings: () => request<SettingsResponse>('GET', '/api/settings'),

  updateSettings: (body: Record<string, string>) =>
    request<SettingsResponse>('PUT', '/api/settings', body),

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
}
