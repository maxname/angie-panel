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
}
