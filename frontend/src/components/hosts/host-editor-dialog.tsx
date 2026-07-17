import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  FileArchive,
  Activity,
  Gauge,
  Globe,
  KeyRound,
  Loader2,
  Lock,
  OctagonAlert,
  Plus,
  Route,
  Server,
  SlidersHorizontal,
  Tags,
  Terminal,
  Wrench,
  X,
  type LucideIcon,
} from 'lucide-react'
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from 'react'
import { useTranslation } from 'react-i18next'

import { DomainChipsField } from '@/components/hosts/host-editor-fields'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
} from '@/components/ui/sidebar'
import { Tabs as TabsPrimitive } from 'radix-ui'

import { Tabs, TabsContent } from '@/components/ui/tabs'
import { Textarea } from '@/components/ui/textarea'
import {
  api,
  ApiError,
  type BalanceMethod,
  type CustomHeader,
  type ForwardScheme,
  type HeaderDirection,
  type HealthCheck,
  type Host,
  type HostInput,
} from '@/lib/api'
import { toast } from '@/lib/toast'

interface LocationDraft {
  path: string
  forward_scheme: ForwardScheme
  forward_host: string
  forward_port: string
  rewrite: string
  snippet: string
}

interface ServerDraft {
  host: string
  port: string
  weight: string
  backup: boolean
  down: boolean
}

interface FormState {
  domains: string[]
  forward_scheme: ForwardScheme
  forward_host: string
  forward_port: string
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
  locations: LocationDraft[]
  advanced_snippet: string
  rate_limit_enabled: boolean
  rate_limit_rps: string
  rate_limit_burst: string
  rate_limit_nodelay: boolean
  rate_limit_conn: string
  balance_method: BalanceMethod
  primary_weight: string
  max_fails: string
  fail_timeout_secs: string
  servers: ServerDraft[]
  mtls_ca_pem: string
  mtls_optional: boolean
  forward_auth_enabled: boolean
  forward_auth_verify_url: string
  forward_auth_sign_in_url: string
  /** Identity headers as free text (comma/space/newline separated). */
  forward_auth_headers: string
  custom_headers: CustomHeader[]
  maintenance_enabled: boolean
  maintenance_title: string
  maintenance_message: string
  gzip_enabled: boolean
  gzip_comp_level: string
  gzip_min_length: string
  /** MIME types as free text (comma/space/newline separated). */
  gzip_types: string
  error_404_enabled: boolean
  error_404_title: string
  error_404_message: string
  error_5xx_enabled: boolean
  error_5xx_title: string
  error_5xx_message: string
  proxy_body_size: string
  proxy_connect_timeout: string
  proxy_read_timeout: string
  proxy_send_timeout: string
  proxy_disable_buffering: boolean
  // Availability checks, flattened per kind like rate_limit. Interval/timeout
  // are strings so empty = inherit the app default.
  health_tcp_enabled: boolean
  health_tcp_interval: string
  health_tcp_timeout: string
  health_tcp_port: string
  health_http_enabled: boolean
  health_http_interval: string
  health_http_timeout: string
  health_http_path: string
  health_http_insecure: boolean
  /** Accepted status codes as free text (comma/space separated). Empty = any 2xx. */
  health_http_expected: string
  health_http_keyword: string
  health_http_keyword_absent: boolean
}

/** '' for null/undefined, the number as a string otherwise — the inverse of the
 *  parse on submit, so an inherited (null) interval stays empty in the form. */
/** Turn the flat form fields back into the HealthCheck[] the API stores. Only
 *  enabled kinds are emitted; a blank interval/timeout/port becomes null so the
 *  host inherits the app default rather than freezing today's value. */
function buildHealthChecks(form: FormState): HealthCheck[] {
  const num = (v: string): number | null => {
    const n = Number.parseInt(v, 10)
    return Number.isFinite(n) && n > 0 ? n : null
  }
  const checks: HealthCheck[] = []
  if (form.health_tcp_enabled) {
    checks.push({
      kind: 'tcp',
      enabled: true,
      interval_secs: num(form.health_tcp_interval),
      timeout_secs: num(form.health_tcp_timeout),
      port: num(form.health_tcp_port),
      // HTTP-only fields, inert for TCP but required by the type.
      path: '',
      expected_status: [],
      keyword: null,
      keyword_absent: false,
      insecure: false,
    })
  }
  if (form.health_http_enabled) {
    checks.push({
      kind: 'http',
      enabled: true,
      interval_secs: num(form.health_http_interval),
      timeout_secs: num(form.health_http_timeout),
      path: form.health_http_path.trim(),
      expected_status: form.health_http_expected
        .split(/[\s,]+/)
        .map((c) => Number.parseInt(c, 10))
        .filter((n) => Number.isFinite(n) && n >= 100 && n <= 599),
      keyword: form.health_http_keyword.trim() || null,
      keyword_absent: form.health_http_keyword_absent,
      insecure: form.health_http_insecure,
      port: null,
    })
  }
  return checks
}

function numStr(n: number | null | undefined): string {
  return n == null ? '' : String(n)
}

function initialState(host: Host | null): FormState {
  const tcp = host?.health_checks.find((c) => c.kind === 'tcp')
  const http = host?.health_checks.find((c) => c.kind === 'http')

  if (host === null) {
    return {
      domains: [],
      forward_scheme: 'http',
      forward_host: '',
      forward_port: '80',
      websockets_upgrade: false,
      block_exploits: false,
      cache_assets: false,
      http2: true,
      http3: false,
      force_ssl: true,
      hsts: false,
      hsts_subdomains: false,
      trust_forwarded_proto: false,
      certificate_id: null,
      access_list_id: null,
      locations: [],
      advanced_snippet: '',
      rate_limit_enabled: false,
      rate_limit_rps: '',
      rate_limit_burst: '',
      rate_limit_nodelay: false,
      rate_limit_conn: '',
      balance_method: 'round_robin',
      primary_weight: '1',
      max_fails: '1',
      fail_timeout_secs: '10',
      servers: [],
      mtls_ca_pem: '',
      mtls_optional: false,
      forward_auth_enabled: false,
      forward_auth_verify_url: '',
      forward_auth_sign_in_url: '',
      forward_auth_headers: '',
      custom_headers: [],
      maintenance_enabled: false,
      maintenance_title: '',
      maintenance_message: '',
      gzip_enabled: false,
      gzip_comp_level: '',
      gzip_min_length: '',
      gzip_types: '',
      error_404_enabled: false,
      error_404_title: '',
      error_404_message: '',
      error_5xx_enabled: false,
      error_5xx_title: '',
      error_5xx_message: '',
      proxy_body_size: '',
      proxy_connect_timeout: '',
      proxy_read_timeout: '',
      proxy_send_timeout: '',
      proxy_disable_buffering: false,
      health_tcp_enabled: false,
      health_tcp_interval: '',
      health_tcp_timeout: '',
      health_tcp_port: '',
      health_http_enabled: false,
      health_http_interval: '',
      health_http_timeout: '',
      health_http_path: '',
      health_http_insecure: false,
      health_http_expected: '',
      health_http_keyword: '',
      health_http_keyword_absent: false,
    }
  }
  const rl = host.rate_limit
  const up = host.upstream
  const pt = host.proxy_tuning
  return {
    domains: [...host.domains],
    forward_scheme: host.forward_scheme,
    forward_host: host.forward_host,
    forward_port: String(host.forward_port),
    websockets_upgrade: host.websockets_upgrade,
    block_exploits: host.block_exploits,
    cache_assets: host.cache_assets,
    http2: host.http2,
    http3: host.http3,
    force_ssl: host.force_ssl,
    hsts: host.hsts,
    hsts_subdomains: host.hsts_subdomains,
    trust_forwarded_proto: host.trust_forwarded_proto,
    certificate_id: host.certificate_id,
    access_list_id: host.access_list_id,
    locations: host.locations.map((location) => ({
      path: location.path,
      forward_scheme: location.forward_scheme,
      forward_host: location.forward_host,
      forward_port: String(location.forward_port),
      rewrite: location.rewrite ?? '',
      snippet: location.snippet ?? '',
    })),
    advanced_snippet: host.advanced_snippet ?? '',
    rate_limit_enabled: rl.enabled,
    rate_limit_rps: rl.rps > 0 ? String(rl.rps) : '',
    rate_limit_burst: rl.burst > 0 ? String(rl.burst) : '',
    rate_limit_nodelay: rl.nodelay,
    rate_limit_conn: rl.conn > 0 ? String(rl.conn) : '',
    balance_method: up.method,
    primary_weight: String(up.primary_weight),
    max_fails: String(up.max_fails),
    fail_timeout_secs: String(up.fail_timeout_secs),
    servers: up.servers.map((s) => ({
      host: s.host,
      port: String(s.port),
      weight: String(s.weight),
      backup: s.backup,
      down: s.down,
    })),
    mtls_ca_pem: host.mtls.ca_pem ?? '',
    mtls_optional: host.mtls.optional,
    forward_auth_enabled: host.forward_auth.enabled,
    forward_auth_verify_url: host.forward_auth.verify_url,
    forward_auth_sign_in_url: host.forward_auth.sign_in_url ?? '',
    forward_auth_headers: host.forward_auth.copy_headers.join(', '),
    custom_headers: host.custom_headers.map((h) => ({ ...h })),
    maintenance_enabled: host.maintenance.enabled,
    maintenance_title: host.maintenance.title,
    maintenance_message: host.maintenance.message,
    gzip_enabled: host.gzip.enabled,
    gzip_comp_level: host.gzip.comp_level > 0 ? String(host.gzip.comp_level) : '',
    gzip_min_length: host.gzip.min_length > 0 ? String(host.gzip.min_length) : '',
    gzip_types: host.gzip.types.join(', '),
    error_404_enabled: host.error_pages.not_found.enabled,
    error_404_title: host.error_pages.not_found.title,
    error_404_message: host.error_pages.not_found.message,
    error_5xx_enabled: host.error_pages.server_error.enabled,
    error_5xx_title: host.error_pages.server_error.title,
    error_5xx_message: host.error_pages.server_error.message,
    proxy_body_size: pt.client_max_body_size,
    proxy_connect_timeout: pt.connect_timeout_secs > 0 ? String(pt.connect_timeout_secs) : '',
    proxy_read_timeout: pt.read_timeout_secs > 0 ? String(pt.read_timeout_secs) : '',
    proxy_send_timeout: pt.send_timeout_secs > 0 ? String(pt.send_timeout_secs) : '',
    proxy_disable_buffering: pt.disable_buffering,
    health_tcp_enabled: tcp?.enabled ?? false,
    health_tcp_interval: numStr(tcp?.interval_secs),
    health_tcp_timeout: numStr(tcp?.timeout_secs),
    health_tcp_port: numStr(tcp?.port),
    health_http_enabled: http?.enabled ?? false,
    health_http_interval: numStr(http?.interval_secs),
    health_http_timeout: numStr(http?.timeout_secs),
    health_http_path: http?.path ?? '',
    health_http_insecure: http?.insecure ?? false,
    health_http_expected: (http?.expected_status ?? []).join(', '),
    health_http_keyword: http?.keyword ?? '',
    health_http_keyword_absent: http?.keyword_absent ?? false,
  }
}

interface HostEditorDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The host being edited, or null when creating a new one. */
  host: Host | null
}

/** The editor's sections, in sidebar order. `key` is both the tab value and the
 * i18n suffix under `hosts.editor.tabs`. */
const EDITOR_SECTIONS: { key: string; Icon: LucideIcon }[] = [
  { key: 'details', Icon: Globe },
  { key: 'ssl', Icon: Lock },
  { key: 'sso', Icon: KeyRound },
  { key: 'locations', Icon: Route },
  { key: 'upstreams', Icon: Server },
  { key: 'rateLimit', Icon: Gauge },
  { key: 'health', Icon: Activity },
  { key: 'proxyTuning', Icon: SlidersHorizontal },
  { key: 'headers', Icon: Tags },
  { key: 'gzip', Icon: FileArchive },
  { key: 'errorPages', Icon: OctagonAlert },
  { key: 'maintenance', Icon: Wrench },
  { key: 'advanced', Icon: Terminal },
]

export function HostEditorDialog({
  open,
  onOpenChange,
  host,
}: HostEditorDialogProps) {
  const { t } = useTranslation()
  // Fed by the form; guards accidental closes (Escape / overlay / ✕ / Cancel)
  // when there are unsaved edits.
  const dirtyRef = useRef(false)

  const guardedClose = useCallback(
    (next: boolean) => {
      if (!next && dirtyRef.current) {
        if (!window.confirm(t('hosts.editor.unsavedConfirm'))) return
      }
      onOpenChange(next)
    },
    [onOpenChange, t],
  )

  const handleDirtyChange = useCallback((dirty: boolean) => {
    dirtyRef.current = dirty
  }, [])

  return (
    <Dialog open={open} onOpenChange={guardedClose}>
      <DialogContent className="gap-0 overflow-hidden p-0 sm:max-w-3xl">
        {/* Accessible title for the dialog; the visible one is in the form. */}
        <DialogHeader className="sr-only">
          <DialogTitle>
            {host === null
              ? t('hosts.editor.createTitle')
              : t('hosts.editor.editTitle')}
          </DialogTitle>
          <DialogDescription>{t('hosts.editor.description')}</DialogDescription>
        </DialogHeader>
        {/* Remount the form whenever the target host changes so state resets. */}
        <HostEditorForm
          key={host?.id ?? 'new'}
          host={host}
          onDirtyChange={handleDirtyChange}
          onCancel={() => guardedClose(false)}
          onDone={() => {
            // Save succeeded — bypass the unsaved-changes guard.
            dirtyRef.current = false
            onOpenChange(false)
          }}
        />
      </DialogContent>
    </Dialog>
  )
}

interface HostEditorFormProps {
  host: Host | null
  onDone: () => void
  /** Called when the "close" action (Cancel) is requested; the parent decides
   *  whether to confirm discarding unsaved changes. */
  onCancel?: () => void
  /** Reports whether the form has unsaved edits, so the parent can guard close. */
  onDirtyChange?: (dirty: boolean) => void
}

export function HostEditorForm({
  host,
  onDone,
  onCancel,
  onDirtyChange,
}: HostEditorFormProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()

  // Kept fresh via the shared ['certificates'] key so newly created certs appear.
  const certsQuery = useQuery({
    queryKey: ['certificates'],
    queryFn: () => api.listCertificates(),
  })
  const certificates = certsQuery.data?.certificates ?? []

  // Shares the ['access-lists'] key so lists created on that page appear here.
  const accessListsQuery = useQuery({
    queryKey: ['access-lists'],
    queryFn: () => api.listAccessLists(),
  })
  const accessLists = accessListsQuery.data?.access_lists ?? []

  const [initialForm] = useState(() => initialState(host))
  const [form, setForm] = useState<FormState>(initialForm)
  const [formError, setFormError] = useState<string | null>(null)
  const [tab, setTab] = useState('details')

  const patch = (partial: Partial<FormState>) =>
    setForm((prev) => ({ ...prev, ...partial }))

  // Track whether the form diverges from its initial state so the dialog can
  // warn before an accidental close (Escape / overlay / ✕) discards edits.
  // Stringifying the whole form per keystroke looks wasteful and isn't:
  // measured at 0.7µs for a typical host, 11.6µs for the heaviest one a user
  // can build (a CA bundle, a 200-line snippet, 20 locations, 30 headers) —
  // against a 16.7ms frame, and dwarfed by the re-render it rides along with.
  // A hand-rolled deep compare would buy nothing and could disagree with the
  // payload we actually send.
  const initialSnapshot = useMemo(() => JSON.stringify(initialForm), [initialForm])
  const isDirty = useMemo(
    () => JSON.stringify(form) !== initialSnapshot,
    [form, initialSnapshot],
  )
  // Only an edit can be "unchanged". A create form starts pristine by
  // definition, and its Save is what tells you which fields are missing.
  // Deliberately not folded into isDirty: that one guards the close warning,
  // and an untouched new host must not ask whether to discard anything.
  const canSave = isDirty || host === null

  useEffect(() => {
    onDirtyChange?.(isDirty)
  }, [isDirty, onDirtyChange])

  const mutation = useMutation({
    mutationFn: (input: HostInput) =>
      host === null ? api.createHost(input) : api.updateHost(host.id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['hosts'] })
      toast({
        title: t('hosts.unappliedTitle'),
        description: t('hosts.unappliedBody'),
      })
      onDone()
    },
  })

  const serverError = useMemo(() => {
    if (!mutation.isError) {
      return null
    }
    if (mutation.error instanceof ApiError) {
      return { code: mutation.error.code, message: mutation.error.message }
    }
    return { code: 'unknown_error', message: t('common.error') }
  }, [mutation.isError, mutation.error, t])

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setFormError(null)

    // Report a validation failure: show the message, jump to its tab, and focus
    // the offending field so the user lands directly on what to fix. The rAF
    // waits for the tab switch to commit the target field to the DOM.
    const fail = (tabName: string, message: string, fieldId?: string) => {
      setFormError(message)
      setTab(tabName)
      if (fieldId) {
        requestAnimationFrame(() =>
          document.getElementById(fieldId)?.focus(),
        )
      }
    }

    if (form.domains.length === 0) {
      fail('details', t('hosts.editor.noDomains'), 'host-domain-input')
      return
    }
    if (form.forward_host.trim() === '') {
      fail('details', t('hosts.editor.noForwardHost'), 'host-forward-host')
      return
    }
    const port = Number.parseInt(form.forward_port, 10)
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      fail('details', t('hosts.editor.invalidPort'), 'host-forward-port')
      return
    }
    const rlRps = Number.parseInt(form.rate_limit_rps, 10) || 0
    const rlConn = Number.parseInt(form.rate_limit_conn, 10) || 0
    if (form.rate_limit_enabled && rlRps <= 0 && rlConn <= 0) {
      fail('rateLimit', t('hosts.editor.rateLimit.errNoLimit'), 'host-rl-rps')
      return
    }

    // Additional upstream servers: each needs a host and a valid port; ip_hash
    // forbids backup peers (Angie rejects the combo).
    for (const [index, s] of form.servers.entries()) {
      const sp = Number.parseInt(s.port, 10)
      if (s.host.trim() === '' || !Number.isInteger(sp) || sp < 1 || sp > 65535) {
        fail(
          'upstreams',
          t('hosts.editor.upstreams.errServer'),
          `host-server-host-${index}`,
        )
        return
      }
      if (form.balance_method === 'ip_hash' && s.backup) {
        fail(
          'upstreams',
          t('hosts.editor.upstreams.errIpHashBackup'),
          `host-server-host-${index}`,
        )
        return
      }
    }

    // Forward auth needs a verification endpoint; catch it here (and jump to the
    // SSO tab) instead of surfacing a generic server error at the bottom.
    if (
      form.forward_auth_enabled &&
      form.forward_auth_verify_url.trim() === ''
    ) {
      fail('sso', t('hosts.editor.forwardAuth.errNoVerifyUrl'), 'host-fa-verify')
      return
    }

    const locations = form.locations.map((location) => ({
      path: location.path.trim(),
      forward_scheme: location.forward_scheme,
      forward_host: location.forward_host.trim(),
      forward_port: Number.parseInt(location.forward_port, 10) || 0,
      rewrite: location.rewrite.trim() === '' ? null : location.rewrite.trim(),
      snippet: location.snippet.trim() === '' ? null : location.snippet,
    }))

    const input: HostInput = {
      domains: form.domains,
      forward_scheme: form.forward_scheme,
      forward_host: form.forward_host.trim(),
      forward_port: port,
      websockets_upgrade: form.websockets_upgrade,
      block_exploits: form.block_exploits,
      cache_assets: form.cache_assets,
      http2: form.http2,
      http3: form.http3,
      force_ssl: form.force_ssl,
      hsts: form.hsts,
      hsts_subdomains: form.hsts_subdomains,
      trust_forwarded_proto: form.trust_forwarded_proto,
      certificate_id: form.certificate_id,
      access_list_id: form.access_list_id,
      locations,
      advanced_snippet:
        form.advanced_snippet.trim() === '' ? null : form.advanced_snippet,
      rate_limit: {
        enabled: form.rate_limit_enabled,
        rps: rlRps,
        burst: Number.parseInt(form.rate_limit_burst, 10) || 0,
        nodelay: form.rate_limit_nodelay,
        conn: rlConn,
      },
      upstream: {
        method: form.balance_method,
        primary_weight: Number.parseInt(form.primary_weight, 10) || 1,
        max_fails: Number.parseInt(form.max_fails, 10) || 0,
        fail_timeout_secs: Number.parseInt(form.fail_timeout_secs, 10) || 10,
        servers: form.servers.map((s) => ({
          host: s.host.trim(),
          port: Number.parseInt(s.port, 10) || 0,
          weight: Number.parseInt(s.weight, 10) || 1,
          backup: s.backup,
          down: s.down,
        })),
      },
      mtls: {
        ca_pem: form.mtls_ca_pem.trim() === '' ? null : form.mtls_ca_pem,
        optional: form.mtls_optional,
      },
      forward_auth: {
        enabled: form.forward_auth_enabled,
        verify_url: form.forward_auth_verify_url.trim(),
        sign_in_url:
          form.forward_auth_sign_in_url.trim() === ''
            ? null
            : form.forward_auth_sign_in_url.trim(),
        copy_headers: form.forward_auth_headers
          .split(/[\s,]+/)
          .map((h) => h.trim())
          .filter(Boolean),
      },
      custom_headers: form.custom_headers
        .filter((h) => h.name.trim() !== '')
        .map((h) => ({
          name: h.name.trim(),
          value: h.value,
          direction: h.direction,
        })),
      maintenance: {
        enabled: form.maintenance_enabled,
        title: form.maintenance_title.trim(),
        message: form.maintenance_message.trim(),
      },
      gzip: {
        enabled: form.gzip_enabled,
        comp_level: Number.parseInt(form.gzip_comp_level, 10) || 0,
        min_length: Number.parseInt(form.gzip_min_length, 10) || 0,
        types: form.gzip_types
          .split(/[\s,]+/)
          .map((tt) => tt.trim())
          .filter(Boolean),
      },
      error_pages: {
        not_found: {
          enabled: form.error_404_enabled,
          title: form.error_404_title.trim(),
          message: form.error_404_message.trim(),
        },
        server_error: {
          enabled: form.error_5xx_enabled,
          title: form.error_5xx_title.trim(),
          message: form.error_5xx_message.trim(),
        },
      },
      proxy_tuning: {
        client_max_body_size: form.proxy_body_size.trim(),
        connect_timeout_secs: Number.parseInt(form.proxy_connect_timeout, 10) || 0,
        read_timeout_secs: Number.parseInt(form.proxy_read_timeout, 10) || 0,
        send_timeout_secs: Number.parseInt(form.proxy_send_timeout, 10) || 0,
        disable_buffering: form.proxy_disable_buffering,
      },
      // Only enabled checks are stored; toggling a kind off drops its config,
      // which matches "empty = not probed". Blank interval/timeout become null
      // so the host keeps inheriting the app default.
      health_checks: buildHealthChecks(form),
      enabled: host === null ? true : host.enabled,
    }

    mutation.mutate(input)
  }

  const addHeader = () =>
    patch({
      custom_headers: [
        ...form.custom_headers,
        { name: '', value: '', direction: 'response' },
      ],
    })
  const updateHeader = (index: number, partial: Partial<CustomHeader>) =>
    patch({
      custom_headers: form.custom_headers.map((h, i) =>
        i === index ? { ...h, ...partial } : h,
      ),
    })
  const removeHeader = (index: number) =>
    patch({
      custom_headers: form.custom_headers.filter((_, i) => i !== index),
    })

  return (
    <form className="flex flex-col overflow-hidden" onSubmit={handleSubmit} noValidate>
      <Tabs
        value={tab}
        onValueChange={setTab}
        orientation="vertical"
        className="min-h-0 flex-1 gap-0"
      >
        {/* The real Sidebar, per shadcn's settings-dialog block, rather than a
            TabsList wearing one's clothes. Radix's tab semantics stay: the list
            and each trigger render *as* the sidebar's own elements, so keyboard
            navigation and aria-controls survive the costume change.

            min-h-0 is load-bearing — SidebarProvider ships min-h-svh for
            page-level use, which inside a dialog braces it open to the full
            viewport. shadcn's block gets away with it by clipping; killing it
            outright also lets the sidebar stretch to the panel's height, so its
            border runs the full side instead of stopping under the last item. */}
        <SidebarProvider
          className="min-h-0 w-full"
          style={{ '--sidebar-width': '13rem' } as React.CSSProperties}
        >
          <Sidebar collapsible="none" className="hidden border-r md:flex">
            <SidebarContent>
              <SidebarGroup>
                <SidebarGroupContent>
                  {/* Radix's primitives, not shadcn's styled TabsList/Trigger:
                      the styled ones carry their own look (a centred pill), and
                      asChild joins classNames as plain strings rather than
                      merging them — so justify-center and justify-start both
                      land on the element and source order picks the winner. It
                      picked centre. Unstyled primitives give the tab semantics
                      and leave the sidebar's own look alone. */}
                  <TabsPrimitive.List asChild>
                    <SidebarMenu>
                      {EDITOR_SECTIONS.map(({ key, Icon }) => (
                        <SidebarMenuItem key={key}>
                          <TabsPrimitive.Trigger value={key} asChild>
                            <SidebarMenuButton isActive={tab === key}>
                              <Icon aria-hidden="true" />
                              <span>{t(`hosts.editor.tabs.${key}`)}</span>
                            </SidebarMenuButton>
                          </TabsPrimitive.Trigger>
                        </SidebarMenuItem>
                      ))}
                    </SidebarMenu>
                  </TabsPrimitive.List>
                </SidebarGroupContent>
              </SidebarGroup>
            </SidebarContent>
          </Sidebar>

          {/* Right column: header + the scrolling panel. The visible title is a
              plain heading so the form renders outside a Dialog (in tests); the
              accessible DialogTitle lives in the parent.

              The height is fixed rather than driven by content: sections range
              from two switches to a dozen fields, and letting the panel size
              itself made the dialog jump every time you moved between them. */}
          <div className="flex h-[70vh] max-h-[560px] min-w-0 flex-1 flex-col">
            <div className="px-6 pt-6 pb-4">
              <h2 className="font-heading text-base leading-none font-medium">
                {host === null
                  ? t('hosts.editor.createTitle')
                  : t('hosts.editor.editTitle')}
              </h2>
              <p className="mt-1.5 text-sm text-muted-foreground">
                {t('hosts.editor.description')}
              </p>
            </div>
            {/* The sidebar is gone below md, so the sections need another way
                across. Without this the dialog is a single section on a phone
                with no way out of it. */}
            <div className="px-6 pb-4 md:hidden">
              <Select value={tab} onValueChange={setTab}>
                <SelectTrigger className="w-full" aria-label={t('hosts.editor.section')}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {EDITOR_SECTIONS.map(({ key, Icon }) => (
                    <SelectItem key={key} value={key}>
                      <Icon aria-hidden="true" />
                      {t(`hosts.editor.tabs.${key}`)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-4">
        <TabsContent value="details" className="space-y-4">
          {/* Keeps the id: submitting with no domains focuses it (see fail()). */}
          <DomainChipsField
            id="host-domain-input"
            domains={form.domains}
            onChange={(domains) => patch({ domains })}
          />

          <div className="grid gap-4 sm:grid-cols-[8rem_1fr_7rem]">
            <div className="space-y-2">
              <Label htmlFor="host-scheme">{t('hosts.editor.forwardScheme')}</Label>
              <Select
                value={form.forward_scheme}
                onValueChange={(value) =>
                  patch({ forward_scheme: value as ForwardScheme })
                }
              >
                <SelectTrigger id="host-scheme">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="http">http</SelectItem>
                  <SelectItem value="https">https</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="host-forward-host">{t('hosts.editor.forwardHost')}</Label>
              <Input
                id="host-forward-host"
                value={form.forward_host}
                placeholder="10.0.0.5"
                onChange={(event) => patch({ forward_host: event.target.value })}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="host-forward-port">{t('hosts.editor.forwardPort')}</Label>
              <Input
                id="host-forward-port"
                type="number"
                min={1}
                max={65535}
                value={form.forward_port}
                onChange={(event) => patch({ forward_port: event.target.value })}
              />
            </div>
          </div>

          <div className="space-y-3 rounded-lg border p-3">
            <ToggleRow
              id="host-websockets"
              label={t('hosts.editor.websockets')}
              checked={form.websockets_upgrade}
              onChange={(checked) => patch({ websockets_upgrade: checked })}
            />
            <ToggleRow
              id="host-block-exploits"
              label={t('hosts.editor.blockExploits')}
              checked={form.block_exploits}
              onChange={(checked) => patch({ block_exploits: checked })}
            />
            <ToggleRow
              id="host-cache-assets"
              label={t('hosts.editor.cacheAssets')}
              checked={form.cache_assets}
              onChange={(checked) => patch({ cache_assets: checked })}
            />
          </div>

          <div className="space-y-2">
            <Label htmlFor="host-access-list">
              {t('hosts.editor.accessList')}
            </Label>
            <Select
              value={
                form.access_list_id === null ? 'none' : String(form.access_list_id)
              }
              onValueChange={(value) =>
                patch({
                  access_list_id:
                    value === 'none' ? null : Number.parseInt(value, 10),
                })
              }
            >
              <SelectTrigger id="host-access-list">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">
                  {t('hosts.editor.accessListNone')}
                </SelectItem>
                {accessLists.map((list) => (
                  <SelectItem key={list.id} value={String(list.id)}>
                    {list.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {accessListsQuery.isError && (
              <p role="alert" className="text-sm text-destructive">
                {t('hosts.editor.accessListLoadFailed')}
              </p>
            )}
            <p className="text-sm text-muted-foreground">
              {t('hosts.editor.accessListHelp')}
            </p>
          </div>
        </TabsContent>

        <TabsContent value="ssl" className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="host-certificate">
              {t('hosts.editor.ssl.certificate')}
            </Label>
            <Select
              value={
                form.certificate_id === null ? 'none' : String(form.certificate_id)
              }
              onValueChange={(value) =>
                patch({
                  certificate_id: value === 'none' ? null : Number.parseInt(value, 10),
                })
              }
            >
              <SelectTrigger id="host-certificate">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="none">
                  {t('hosts.editor.ssl.certificateNone')}
                </SelectItem>
                {certificates.map((cert) => (
                  <SelectItem key={cert.id} value={String(cert.id)}>
                    {cert.name} — {cert.domains.join(', ')}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {certsQuery.isError && (
              <p role="alert" className="text-sm text-destructive">
                {t('hosts.editor.ssl.loadFailed')}
              </p>
            )}
            <p className="text-sm text-muted-foreground">
              {form.certificate_id === null
                ? t('hosts.editor.ssl.selectNote')
                : t('hosts.editor.ssl.activeNote')}
            </p>
          </div>
          <div className="space-y-3 rounded-lg border p-3">
            <ToggleRow
              id="host-force-ssl"
              label={t('hosts.editor.forceSsl')}
              checked={form.force_ssl}
              onChange={(checked) => patch({ force_ssl: checked })}
            />
            <ToggleRow
              id="host-http2"
              label={t('hosts.editor.http2')}
              checked={form.http2}
              onChange={(checked) => patch({ http2: checked })}
            />
            <ToggleRow
              id="host-http3"
              label={t('hosts.editor.http3')}
              checked={form.http3}
              onChange={(checked) => patch({ http3: checked })}
            />
            <ToggleRow
              id="host-hsts"
              label={t('hosts.editor.hsts')}
              checked={form.hsts}
              onChange={(checked) => patch({ hsts: checked })}
            />
            <ToggleRow
              id="host-hsts-subdomains"
              label={t('hosts.editor.hstsSubdomains')}
              checked={form.hsts_subdomains}
              onChange={(checked) => patch({ hsts_subdomains: checked })}
            />
            <ToggleRow
              id="host-trust-proto"
              label={t('hosts.editor.trustForwardedProto')}
              checked={form.trust_forwarded_proto}
              onChange={(checked) => patch({ trust_forwarded_proto: checked })}
            />
          </div>

          {/* Mutual TLS: require client certificates verified against a CA. */}
          <div className="space-y-3 rounded-lg border p-3">
            <div className="space-y-1">
              <span className="text-sm font-medium">
                {t('hosts.editor.mtls.title')}
              </span>
              <p className="text-xs text-muted-foreground">
                {t('hosts.editor.mtls.description')}
              </p>
            </div>
            <Textarea
              id="host-mtls-ca"
              className="min-h-28 font-mono text-xs"
              spellCheck={false}
              placeholder="-----BEGIN CERTIFICATE-----&#10;…&#10;-----END CERTIFICATE-----"
              value={form.mtls_ca_pem}
              onChange={(event) => patch({ mtls_ca_pem: event.target.value })}
            />
            {form.mtls_ca_pem.trim() !== '' && (
              <ToggleRow
                id="host-mtls-optional"
                label={t('hosts.editor.mtls.optional')}
                checked={form.mtls_optional}
                onChange={(checked) => patch({ mtls_optional: checked })}
              />
            )}
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.mtls.hint')}
            </p>
          </div>
        </TabsContent>

        <TabsContent value="sso" className="space-y-4">
          {/* Forward auth (SSO gateway) via auth_request. */}
          <div className="space-y-1">
            <span className="text-sm font-medium">
              {t('hosts.editor.forwardAuth.title')}
            </span>
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.forwardAuth.description')}
            </p>
          </div>
          <ToggleRow
            id="host-forward-auth"
            label={t('hosts.editor.forwardAuth.enable')}
            checked={form.forward_auth_enabled}
            onChange={(checked) => patch({ forward_auth_enabled: checked })}
          />
          {form.forward_auth_enabled && (
            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="host-fa-verify">
                  {t('hosts.editor.forwardAuth.verifyUrl')}
                </Label>
                <Input
                  id="host-fa-verify"
                  className="font-mono text-xs"
                  spellCheck={false}
                  placeholder="http://10.0.0.9:9091/api/verify"
                  value={form.forward_auth_verify_url}
                  onChange={(event) =>
                    patch({ forward_auth_verify_url: event.target.value })
                  }
                />
                <p className="text-xs text-muted-foreground">
                  {t('hosts.editor.forwardAuth.verifyUrlHint')}
                </p>
              </div>
              <div className="space-y-2">
                <Label htmlFor="host-fa-signin">
                  {t('hosts.editor.forwardAuth.signInUrl')}
                </Label>
                <Input
                  id="host-fa-signin"
                  className="font-mono text-xs"
                  spellCheck={false}
                  placeholder="https://auth.example.com"
                  value={form.forward_auth_sign_in_url}
                  onChange={(event) =>
                    patch({ forward_auth_sign_in_url: event.target.value })
                  }
                />
                <p className="text-xs text-muted-foreground">
                  {t('hosts.editor.forwardAuth.signInUrlHint')}
                </p>
              </div>
              <div className="space-y-2">
                <Label htmlFor="host-fa-headers">
                  {t('hosts.editor.forwardAuth.headers')}
                </Label>
                <Input
                  id="host-fa-headers"
                  className="font-mono text-xs"
                  spellCheck={false}
                  placeholder="Remote-User, Remote-Groups, Remote-Email"
                  value={form.forward_auth_headers}
                  onChange={(event) =>
                    patch({ forward_auth_headers: event.target.value })
                  }
                />
                <p className="text-xs text-muted-foreground">
                  {t('hosts.editor.forwardAuth.headersHint')}
                </p>
              </div>
            </div>
          )}
        </TabsContent>

        <TabsContent value="locations" className="space-y-4">
          {form.locations.length === 0 && (
            <p className="text-sm text-muted-foreground">
              {t('hosts.editor.locations.empty')}
            </p>
          )}
          {form.locations.map((location, index) => (
            <div
              key={index}
              className="space-y-3 rounded-lg border p-3"
            >
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">
                  {t('hosts.editor.locations.item', { index: index + 1 })}
                </span>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  onClick={() =>
                    patch({
                      locations: form.locations.filter((_, i) => i !== index),
                    })
                  }
                  aria-label={t('hosts.editor.locations.remove')}
                >
                  <X aria-hidden="true" />
                </Button>
              </div>
              <div className="space-y-2">
                <Label htmlFor={`loc-path-${index}`}>
                  {t('hosts.editor.locations.path')}
                </Label>
                <Input
                  id={`loc-path-${index}`}
                  value={location.path}
                  placeholder="/api"
                  onChange={(event) =>
                    patch({
                      locations: form.locations.map((item, i) =>
                        i === index ? { ...item, path: event.target.value } : item,
                      ),
                    })
                  }
                />
              </div>
              <div className="grid gap-3 sm:grid-cols-[8rem_1fr_7rem]">
                <div className="space-y-2">
                  <Label htmlFor={`loc-scheme-${index}`}>
                    {t('hosts.editor.forwardScheme')}
                  </Label>
                  <Select
                    value={location.forward_scheme}
                    onValueChange={(value) =>
                      patch({
                        locations: form.locations.map((item, i) =>
                          i === index
                            ? { ...item, forward_scheme: value as ForwardScheme }
                            : item,
                        ),
                      })
                    }
                  >
                    <SelectTrigger id={`loc-scheme-${index}`}>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="http">http</SelectItem>
                      <SelectItem value="https">https</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <div className="space-y-2">
                  <Label htmlFor={`loc-host-${index}`}>
                    {t('hosts.editor.forwardHost')}
                  </Label>
                  <Input
                    id={`loc-host-${index}`}
                    value={location.forward_host}
                    onChange={(event) =>
                      patch({
                        locations: form.locations.map((item, i) =>
                          i === index
                            ? { ...item, forward_host: event.target.value }
                            : item,
                        ),
                      })
                    }
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor={`loc-port-${index}`}>
                    {t('hosts.editor.forwardPort')}
                  </Label>
                  <Input
                    id={`loc-port-${index}`}
                    type="number"
                    min={1}
                    max={65535}
                    value={location.forward_port}
                    onChange={(event) =>
                      patch({
                        locations: form.locations.map((item, i) =>
                          i === index
                            ? { ...item, forward_port: event.target.value }
                            : item,
                        ),
                      })
                    }
                  />
                </div>
              </div>
              <div className="space-y-2">
                <Label htmlFor={`loc-rewrite-${index}`}>
                  {t('hosts.editor.locations.rewrite')}
                </Label>
                <Input
                  id={`loc-rewrite-${index}`}
                  value={location.rewrite}
                  placeholder="/ /$1 break"
                  onChange={(event) =>
                    patch({
                      locations: form.locations.map((item, i) =>
                        i === index ? { ...item, rewrite: event.target.value } : item,
                      ),
                    })
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor={`loc-snippet-${index}`}>
                  {t('hosts.editor.locations.snippet')}
                </Label>
                <Textarea
                  id={`loc-snippet-${index}`}
                  className="min-h-20 font-mono text-xs"
                  value={location.snippet}
                  spellCheck={false}
                  onChange={(event) =>
                    patch({
                      locations: form.locations.map((item, i) =>
                        i === index ? { ...item, snippet: event.target.value } : item,
                      ),
                    })
                  }
                />
              </div>
            </div>
          ))}
          <Button
            type="button"
            variant="outline"
            onClick={() =>
              patch({
                locations: [
                  ...form.locations,
                  {
                    path: '',
                    forward_scheme: 'http',
                    forward_host: '',
                    forward_port: '80',
                    rewrite: '',
                    snippet: '',
                  },
                ],
              })
            }
          >
            <Plus aria-hidden="true" />
            {t('hosts.editor.locations.add')}
          </Button>
        </TabsContent>

        <TabsContent value="upstreams" className="space-y-4">
          <p className="text-sm text-muted-foreground">
            {t('hosts.editor.upstreams.description')}
          </p>

          <div className="grid gap-4 sm:grid-cols-[1fr_8rem]">
            <div className="space-y-2">
              <Label htmlFor="host-balance-method">
                {t('hosts.editor.upstreams.method')}
              </Label>
              <Select
                value={form.balance_method}
                onValueChange={(value) =>
                  patch({ balance_method: value as BalanceMethod })
                }
              >
                <SelectTrigger id="host-balance-method">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="round_robin">
                    {t('hosts.editor.upstreams.roundRobin')}
                  </SelectItem>
                  <SelectItem value="least_conn">least_conn</SelectItem>
                  <SelectItem value="ip_hash">ip_hash</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          {/* Server pool: the primary (from Details) plus additional peers. */}
          <div className="space-y-3 rounded-lg border p-3">
            <div className="flex items-center justify-between">
              <span className="text-sm font-medium">
                {t('hosts.editor.upstreams.servers')}
              </span>
            </div>
            <div className="flex items-center gap-2 text-sm">
              <span className="rounded bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                {t('hosts.editor.upstreams.primary')}
              </span>
              <span className="font-mono text-xs">
                {form.forward_host || '—'}:{form.forward_port || '—'}
              </span>
              <div className="ml-auto flex items-center gap-2">
                <Label
                  htmlFor="host-primary-weight"
                  className="text-xs font-normal text-muted-foreground"
                >
                  {t('hosts.editor.upstreams.weight')}
                </Label>
                <Input
                  id="host-primary-weight"
                  inputMode="numeric"
                  className="h-8 w-16"
                  value={form.primary_weight}
                  onChange={(event) =>
                    patch({
                      primary_weight: event.target.value.replace(/[^0-9]/g, ''),
                    })
                  }
                />
              </div>
            </div>

            {form.servers.map((server, index) => (
              <div
                key={index}
                className="flex flex-wrap items-center gap-2 border-t pt-3"
              >
                <Input
                  id={`host-server-host-${index}`}
                  aria-label={t('hosts.editor.upstreams.serverHost')}
                  placeholder="10.0.0.2"
                  className="h-8 w-40"
                  value={server.host}
                  onChange={(event) =>
                    patch({
                      servers: form.servers.map((s, i) =>
                        i === index ? { ...s, host: event.target.value } : s,
                      ),
                    })
                  }
                />
                <Input
                  aria-label={t('hosts.editor.upstreams.serverPort')}
                  inputMode="numeric"
                  placeholder="8080"
                  className="h-8 w-20"
                  value={server.port}
                  onChange={(event) =>
                    patch({
                      servers: form.servers.map((s, i) =>
                        i === index
                          ? { ...s, port: event.target.value.replace(/[^0-9]/g, '') }
                          : s,
                      ),
                    })
                  }
                />
                <Input
                  aria-label={t('hosts.editor.upstreams.weight')}
                  inputMode="numeric"
                  placeholder="1"
                  className="h-8 w-14"
                  value={server.weight}
                  onChange={(event) =>
                    patch({
                      servers: form.servers.map((s, i) =>
                        i === index
                          ? { ...s, weight: event.target.value.replace(/[^0-9]/g, '') }
                          : s,
                      ),
                    })
                  }
                />
                <label className="flex items-center gap-1 text-xs">
                  <input
                    type="checkbox"
                    checked={server.backup}
                    onChange={(event) =>
                      patch({
                        servers: form.servers.map((s, i) =>
                          i === index ? { ...s, backup: event.target.checked } : s,
                        ),
                      })
                    }
                  />
                  {t('hosts.editor.upstreams.backup')}
                </label>
                <label className="flex items-center gap-1 text-xs">
                  <input
                    type="checkbox"
                    checked={server.down}
                    onChange={(event) =>
                      patch({
                        servers: form.servers.map((s, i) =>
                          i === index ? { ...s, down: event.target.checked } : s,
                        ),
                      })
                    }
                  />
                  {t('hosts.editor.upstreams.down')}
                </label>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  className="ml-auto"
                  aria-label={t('hosts.editor.upstreams.removeServer')}
                  onClick={() =>
                    patch({
                      servers: form.servers.filter((_, i) => i !== index),
                    })
                  }
                >
                  <X aria-hidden="true" />
                </Button>
              </div>
            ))}

            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() =>
                patch({
                  servers: [
                    ...form.servers,
                    { host: '', port: '', weight: '1', backup: false, down: false },
                  ],
                })
              }
            >
              <Plus aria-hidden="true" />
              {t('hosts.editor.upstreams.addServer')}
            </Button>
          </div>

          {/* Passive health checks. */}
          <div className="space-y-3 rounded-lg border p-3">
            <span className="text-sm font-medium">
              {t('hosts.editor.upstreams.health')}
            </span>
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label htmlFor="host-max-fails">
                  {t('hosts.editor.upstreams.maxFails')}
                </Label>
                <Input
                  id="host-max-fails"
                  inputMode="numeric"
                  value={form.max_fails}
                  onChange={(event) =>
                    patch({ max_fails: event.target.value.replace(/[^0-9]/g, '') })
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="host-fail-timeout">
                  {t('hosts.editor.upstreams.failTimeout')}
                </Label>
                <Input
                  id="host-fail-timeout"
                  inputMode="numeric"
                  value={form.fail_timeout_secs}
                  onChange={(event) =>
                    patch({
                      fail_timeout_secs: event.target.value.replace(/[^0-9]/g, ''),
                    })
                  }
                />
              </div>
            </div>
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.upstreams.healthHint')}
            </p>
          </div>
        </TabsContent>

        <TabsContent value="rateLimit" className="space-y-4">
          <p className="text-sm text-muted-foreground">
            {t('hosts.editor.rateLimit.description')}
          </p>
          <div className="space-y-3 rounded-lg border p-3">
            <ToggleRow
              id="host-rate-limit-enabled"
              label={t('hosts.editor.rateLimit.enable')}
              checked={form.rate_limit_enabled}
              onChange={(checked) => patch({ rate_limit_enabled: checked })}
            />
          </div>
          {form.rate_limit_enabled && (
            <div className="space-y-4 rounded-lg border p-3">
              <div className="grid gap-4 sm:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="host-rl-rps">
                    {t('hosts.editor.rateLimit.rps')}
                  </Label>
                  <Input
                    id="host-rl-rps"
                    inputMode="numeric"
                    placeholder="10"
                    value={form.rate_limit_rps}
                    onChange={(event) =>
                      patch({
                        rate_limit_rps: event.target.value.replace(/[^0-9]/g, ''),
                      })
                    }
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="host-rl-burst">
                    {t('hosts.editor.rateLimit.burst')}
                  </Label>
                  <Input
                    id="host-rl-burst"
                    inputMode="numeric"
                    placeholder="20"
                    value={form.rate_limit_burst}
                    onChange={(event) =>
                      patch({
                        rate_limit_burst: event.target.value.replace(/[^0-9]/g, ''),
                      })
                    }
                  />
                </div>
              </div>
              <ToggleRow
                id="host-rl-nodelay"
                label={t('hosts.editor.rateLimit.nodelay')}
                checked={form.rate_limit_nodelay}
                onChange={(checked) => patch({ rate_limit_nodelay: checked })}
              />
              <div className="space-y-2 sm:max-w-[16rem]">
                <Label htmlFor="host-rl-conn">
                  {t('hosts.editor.rateLimit.conn')}
                </Label>
                <Input
                  id="host-rl-conn"
                  inputMode="numeric"
                  placeholder="0"
                  value={form.rate_limit_conn}
                  onChange={(event) =>
                    patch({
                      rate_limit_conn: event.target.value.replace(/[^0-9]/g, ''),
                    })
                  }
                />
              </div>
              <p className="text-xs text-muted-foreground">
                {t('hosts.editor.rateLimit.hint')}
              </p>
            </div>
          )}
        </TabsContent>

        <TabsContent value="health" className="space-y-4">
          <p className="text-sm text-muted-foreground">
            {t('hosts.editor.health.description')}
          </p>

          {/* TCP — is the backend listening. */}
          <div className="space-y-3 rounded-lg border p-3">
            <ToggleRow
              id="host-health-tcp"
              label={t('hosts.editor.health.tcpEnable')}
              checked={form.health_tcp_enabled}
              onChange={(checked) => patch({ health_tcp_enabled: checked })}
            />
            {form.health_tcp_enabled && (
              <div className="grid gap-4 sm:grid-cols-3">
                <HealthNumber
                  id="host-health-tcp-port"
                  label={t('hosts.editor.health.tcpPort')}
                  placeholder={String(form.forward_port || t('hosts.editor.health.samePort'))}
                  value={form.health_tcp_port}
                  onChange={(v) => patch({ health_tcp_port: v })}
                />
                <HealthNumber
                  id="host-health-tcp-interval"
                  label={t('hosts.editor.health.interval')}
                  placeholder={t('hosts.editor.health.inherit')}
                  value={form.health_tcp_interval}
                  onChange={(v) => patch({ health_tcp_interval: v })}
                />
                <HealthNumber
                  id="host-health-tcp-timeout"
                  label={t('hosts.editor.health.timeout')}
                  placeholder={t('hosts.editor.health.inherit')}
                  value={form.health_tcp_timeout}
                  onChange={(v) => patch({ health_tcp_timeout: v })}
                />
              </div>
            )}
          </div>

          {/* HTTP — does the whole chain serve. */}
          <div className="space-y-3 rounded-lg border p-3">
            <ToggleRow
              id="host-health-http"
              label={t('hosts.editor.health.httpEnable')}
              checked={form.health_http_enabled}
              onChange={(checked) => patch({ health_http_enabled: checked })}
            />
            {form.health_http_enabled && (
              <div className="space-y-4">
                <div className="grid gap-4 sm:grid-cols-3">
                  <div className="space-y-2">
                    <Label htmlFor="host-health-http-path">
                      {t('hosts.editor.health.path')}
                    </Label>
                    <Input
                      id="host-health-http-path"
                      placeholder="/"
                      value={form.health_http_path}
                      onChange={(e) => patch({ health_http_path: e.target.value })}
                    />
                  </div>
                  <HealthNumber
                    id="host-health-http-interval"
                    label={t('hosts.editor.health.interval')}
                    placeholder={t('hosts.editor.health.inherit')}
                    value={form.health_http_interval}
                    onChange={(v) => patch({ health_http_interval: v })}
                  />
                  <HealthNumber
                    id="host-health-http-timeout"
                    label={t('hosts.editor.health.timeout')}
                    placeholder={t('hosts.editor.health.inherit')}
                    value={form.health_http_timeout}
                    onChange={(v) => patch({ health_http_timeout: v })}
                  />
                </div>

                {/* The rest is rarely touched — a plain 2xx check is the norm —
                    so it sits behind a disclosure rather than crowding the tab. */}
                <details className="rounded-md border px-3 py-2 text-sm [&_summary]:cursor-pointer">
                  <summary className="text-muted-foreground">
                    {t('hosts.editor.health.advanced')}
                  </summary>
                  <div className="mt-3 space-y-4">
                    <div className="space-y-2">
                      <Label htmlFor="host-health-http-status">
                        {t('hosts.editor.health.expectedStatus')}
                      </Label>
                      <Input
                        id="host-health-http-status"
                        placeholder="200, 204"
                        value={form.health_http_expected}
                        onChange={(e) => patch({ health_http_expected: e.target.value })}
                      />
                      <p className="text-xs text-muted-foreground">
                        {t('hosts.editor.health.expectedStatusHint')}
                      </p>
                    </div>
                    <div className="space-y-2">
                      <Label htmlFor="host-health-http-keyword">
                        {t('hosts.editor.health.keyword')}
                      </Label>
                      <Input
                        id="host-health-http-keyword"
                        value={form.health_http_keyword}
                        onChange={(e) => patch({ health_http_keyword: e.target.value })}
                      />
                      {form.health_http_keyword.trim() !== '' && (
                        <ToggleRow
                          id="host-health-http-keyword-absent"
                          label={t('hosts.editor.health.keywordAbsent')}
                          checked={form.health_http_keyword_absent}
                          onChange={(c) => patch({ health_http_keyword_absent: c })}
                        />
                      )}
                    </div>
                    <ToggleRow
                      id="host-health-http-insecure"
                      label={t('hosts.editor.health.insecure')}
                      checked={form.health_http_insecure}
                      onChange={(c) => patch({ health_http_insecure: c })}
                    />
                  </div>
                </details>
              </div>
            )}
          </div>
        </TabsContent>

        <TabsContent value="headers" className="space-y-4">
          <div className="space-y-1">
            <span className="text-sm font-medium">
              {t('hosts.editor.headers.title')}
            </span>
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.headers.description')}
            </p>
          </div>
          {form.custom_headers.length === 0 && (
            <p className="text-sm text-muted-foreground">
              {t('hosts.editor.headers.empty')}
            </p>
          )}
          {form.custom_headers.map((header, index) => (
            <div
              key={index}
              className="flex flex-col gap-2 rounded-lg border p-3 sm:flex-row sm:items-end"
            >
              <div className="flex-1 space-y-1">
                <Label htmlFor={`host-header-name-${index}`}>
                  {t('hosts.editor.headers.name')}
                </Label>
                <Input
                  id={`host-header-name-${index}`}
                  className="font-mono text-xs"
                  spellCheck={false}
                  placeholder="X-Frame-Options"
                  value={header.name}
                  onChange={(event) =>
                    updateHeader(index, { name: event.target.value })
                  }
                />
              </div>
              <div className="flex-1 space-y-1">
                <Label htmlFor={`host-header-value-${index}`}>
                  {t('hosts.editor.headers.value')}
                </Label>
                <Input
                  id={`host-header-value-${index}`}
                  className="font-mono text-xs"
                  spellCheck={false}
                  placeholder="SAMEORIGIN"
                  value={header.value}
                  onChange={(event) =>
                    updateHeader(index, { value: event.target.value })
                  }
                />
              </div>
              <div className="space-y-1">
                <Label htmlFor={`host-header-dir-${index}`}>
                  {t('hosts.editor.headers.direction')}
                </Label>
                <Select
                  value={header.direction}
                  onValueChange={(value) =>
                    updateHeader(index, { direction: value as HeaderDirection })
                  }
                >
                  <SelectTrigger
                    id={`host-header-dir-${index}`}
                    className="w-full sm:w-40"
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="response">
                      {t('hosts.editor.headers.response')}
                    </SelectItem>
                    <SelectItem value="request">
                      {t('hosts.editor.headers.request')}
                    </SelectItem>
                  </SelectContent>
                </Select>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label={t('hosts.editor.headers.remove')}
                onClick={() => removeHeader(index)}
              >
                <X aria-hidden="true" />
              </Button>
            </div>
          ))}
          <Button type="button" variant="outline" size="sm" onClick={addHeader}>
            <Plus aria-hidden="true" />
            {t('hosts.editor.headers.add')}
          </Button>
        </TabsContent>

        <TabsContent value="proxyTuning" className="space-y-4">
          <div className="space-y-1">
            <span className="text-sm font-medium">
              {t('hosts.editor.proxyTuning.title')}
            </span>
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.proxyTuning.description')}
            </p>
          </div>
          <div className="space-y-2">
            <Label htmlFor="host-proxy-body">
              {t('hosts.editor.proxyTuning.bodySize')}
            </Label>
            <Input
              id="host-proxy-body"
              className="font-mono text-xs"
              spellCheck={false}
              placeholder="50m"
              value={form.proxy_body_size}
              onChange={(event) => patch({ proxy_body_size: event.target.value })}
            />
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.proxyTuning.bodySizeHint')}
            </p>
          </div>
          <div className="grid gap-4 sm:grid-cols-3">
            <div className="space-y-2">
              <Label htmlFor="host-proxy-connect">
                {t('hosts.editor.proxyTuning.connectTimeout')}
              </Label>
              <Input
                id="host-proxy-connect"
                inputMode="numeric"
                placeholder="—"
                value={form.proxy_connect_timeout}
                onChange={(event) =>
                  patch({
                    proxy_connect_timeout: event.target.value.replace(/[^0-9]/g, ''),
                  })
                }
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="host-proxy-read">
                {t('hosts.editor.proxyTuning.readTimeout')}
              </Label>
              <Input
                id="host-proxy-read"
                inputMode="numeric"
                placeholder="—"
                value={form.proxy_read_timeout}
                onChange={(event) =>
                  patch({
                    proxy_read_timeout: event.target.value.replace(/[^0-9]/g, ''),
                  })
                }
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="host-proxy-send">
                {t('hosts.editor.proxyTuning.sendTimeout')}
              </Label>
              <Input
                id="host-proxy-send"
                inputMode="numeric"
                placeholder="—"
                value={form.proxy_send_timeout}
                onChange={(event) =>
                  patch({
                    proxy_send_timeout: event.target.value.replace(/[^0-9]/g, ''),
                  })
                }
              />
            </div>
          </div>
          <p className="text-xs text-muted-foreground">
            {t('hosts.editor.proxyTuning.timeoutHint')}
          </p>
          <ToggleRow
            id="host-proxy-buffering"
            label={t('hosts.editor.proxyTuning.disableBuffering')}
            checked={form.proxy_disable_buffering}
            onChange={(checked) => patch({ proxy_disable_buffering: checked })}
          />
        </TabsContent>

        <TabsContent value="gzip" className="space-y-4">
          <div className="space-y-1">
            <span className="text-sm font-medium">
              {t('hosts.editor.gzip.title')}
            </span>
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.gzip.description')}
            </p>
          </div>
          <ToggleRow
            id="host-gzip"
            label={t('hosts.editor.gzip.enable')}
            checked={form.gzip_enabled}
            onChange={(checked) => patch({ gzip_enabled: checked })}
          />
          {form.gzip_enabled && (
            <div className="space-y-4">
              <div className="grid gap-4 sm:grid-cols-2">
                <div className="space-y-2">
                  <Label htmlFor="host-gzip-level">
                    {t('hosts.editor.gzip.level')}
                  </Label>
                  <Input
                    id="host-gzip-level"
                    inputMode="numeric"
                    placeholder="6"
                    value={form.gzip_comp_level}
                    onChange={(event) =>
                      patch({
                        gzip_comp_level: event.target.value.replace(/[^0-9]/g, ''),
                      })
                    }
                  />
                  <p className="text-xs text-muted-foreground">
                    {t('hosts.editor.gzip.levelHint')}
                  </p>
                </div>
                <div className="space-y-2">
                  <Label htmlFor="host-gzip-min">
                    {t('hosts.editor.gzip.minLength')}
                  </Label>
                  <Input
                    id="host-gzip-min"
                    inputMode="numeric"
                    placeholder="256"
                    value={form.gzip_min_length}
                    onChange={(event) =>
                      patch({
                        gzip_min_length: event.target.value.replace(/[^0-9]/g, ''),
                      })
                    }
                  />
                  <p className="text-xs text-muted-foreground">
                    {t('hosts.editor.gzip.minLengthHint')}
                  </p>
                </div>
              </div>
              <div className="space-y-2">
                <Label htmlFor="host-gzip-types">
                  {t('hosts.editor.gzip.types')}
                </Label>
                <Input
                  id="host-gzip-types"
                  className="font-mono text-xs"
                  spellCheck={false}
                  placeholder="text/css, application/json, application/javascript"
                  value={form.gzip_types}
                  onChange={(event) => patch({ gzip_types: event.target.value })}
                />
                <p className="text-xs text-muted-foreground">
                  {t('hosts.editor.gzip.typesHint')}
                </p>
              </div>
            </div>
          )}
        </TabsContent>

        <TabsContent value="errorPages" className="space-y-4">
          <div className="space-y-1">
            <span className="text-sm font-medium">
              {t('hosts.editor.errorPages.title')}
            </span>
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.errorPages.description')}
            </p>
          </div>

          <div className="space-y-4 rounded-lg border p-4">
            <ToggleRow
              id="host-error-404"
              label={t('hosts.editor.errorPages.notFound.enable')}
              checked={form.error_404_enabled}
              onChange={(checked) => patch({ error_404_enabled: checked })}
            />
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.errorPages.notFound.hint')}
            </p>
            {form.error_404_enabled && (
              <div className="space-y-3">
                <div className="space-y-2">
                  <Label htmlFor="host-error-404-title">
                    {t('hosts.editor.errorPages.pageTitle')}
                  </Label>
                  <Input
                    id="host-error-404-title"
                    placeholder={t('hosts.editor.errorPages.notFound.titlePlaceholder')}
                    value={form.error_404_title}
                    onChange={(event) =>
                      patch({ error_404_title: event.target.value })
                    }
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="host-error-404-message">
                    {t('hosts.editor.errorPages.message')}
                  </Label>
                  <Textarea
                    id="host-error-404-message"
                    className="min-h-20"
                    placeholder={t('hosts.editor.errorPages.notFound.messagePlaceholder')}
                    value={form.error_404_message}
                    onChange={(event) =>
                      patch({ error_404_message: event.target.value })
                    }
                  />
                </div>
              </div>
            )}
          </div>

          <div className="space-y-4 rounded-lg border p-4">
            <ToggleRow
              id="host-error-5xx"
              label={t('hosts.editor.errorPages.serverError.enable')}
              checked={form.error_5xx_enabled}
              onChange={(checked) => patch({ error_5xx_enabled: checked })}
            />
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.errorPages.serverError.hint')}
            </p>
            {form.error_5xx_enabled && (
              <div className="space-y-3">
                <div className="space-y-2">
                  <Label htmlFor="host-error-5xx-title">
                    {t('hosts.editor.errorPages.pageTitle')}
                  </Label>
                  <Input
                    id="host-error-5xx-title"
                    placeholder={t('hosts.editor.errorPages.serverError.titlePlaceholder')}
                    value={form.error_5xx_title}
                    onChange={(event) =>
                      patch({ error_5xx_title: event.target.value })
                    }
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="host-error-5xx-message">
                    {t('hosts.editor.errorPages.message')}
                  </Label>
                  <Textarea
                    id="host-error-5xx-message"
                    className="min-h-20"
                    placeholder={t('hosts.editor.errorPages.serverError.messagePlaceholder')}
                    value={form.error_5xx_message}
                    onChange={(event) =>
                      patch({ error_5xx_message: event.target.value })
                    }
                  />
                </div>
              </div>
            )}
          </div>
        </TabsContent>

        <TabsContent value="maintenance" className="space-y-4">
          <div className="space-y-1">
            <span className="text-sm font-medium">
              {t('hosts.editor.maintenance.title')}
            </span>
            <p className="text-xs text-muted-foreground">
              {t('hosts.editor.maintenance.description')}
            </p>
          </div>
          <ToggleRow
            id="host-maintenance"
            label={t('hosts.editor.maintenance.enable')}
            checked={form.maintenance_enabled}
            onChange={(checked) => patch({ maintenance_enabled: checked })}
          />
          {form.maintenance_enabled && (
            <>
              <Alert variant="warning">
                <AlertDescription>
                  {t('hosts.editor.maintenance.warning')}
                </AlertDescription>
              </Alert>
              <div className="space-y-2">
                <Label htmlFor="host-maintenance-title">
                  {t('hosts.editor.maintenance.pageTitle')}
                </Label>
                <Input
                  id="host-maintenance-title"
                  placeholder={t('hosts.editor.maintenance.pageTitlePlaceholder')}
                  value={form.maintenance_title}
                  onChange={(event) =>
                    patch({ maintenance_title: event.target.value })
                  }
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="host-maintenance-message">
                  {t('hosts.editor.maintenance.message')}
                </Label>
                <Textarea
                  id="host-maintenance-message"
                  className="min-h-20"
                  placeholder={t('hosts.editor.maintenance.messagePlaceholder')}
                  value={form.maintenance_message}
                  onChange={(event) =>
                    patch({ maintenance_message: event.target.value })
                  }
                />
                <p className="text-xs text-muted-foreground">
                  {t('hosts.editor.maintenance.hint')}
                </p>
              </div>
            </>
          )}
        </TabsContent>

        <TabsContent value="advanced" className="space-y-4">
          <Alert variant="destructive">
            <AlertTitle>{t('hosts.editor.advanced.warningTitle')}</AlertTitle>
            <AlertDescription>
              {t('hosts.editor.advanced.warningBody')}
            </AlertDescription>
          </Alert>
          <div className="space-y-2">
            <Label htmlFor="host-advanced-snippet">
              {t('hosts.editor.advanced.label')}
            </Label>
            <Textarea
              id="host-advanced-snippet"
              className="min-h-40 font-mono text-xs"
              value={form.advanced_snippet}
              spellCheck={false}
              onChange={(event) => patch({ advanced_snippet: event.target.value })}
            />
          </div>
        </TabsContent>
          </div>
          </div>
        </SidebarProvider>
      </Tabs>

      <div className="space-y-3 border-t px-6 py-4">
        {formError !== null && (
          <p role="alert" className="text-sm text-destructive">
            {formError}
          </p>
        )}
        {serverError !== null && (
          <Alert variant="destructive">
            <AlertTitle>{t('hosts.editor.saveFailed')}</AlertTitle>
            <AlertDescription>{serverError.message}</AlertDescription>
          </Alert>
        )}
        <DialogFooter>
          <Button
            type="button"
            variant="outline"
            onClick={onCancel ?? onDone}
            disabled={mutation.isPending}
          >
            {t('common.cancel')}
          </Button>
          <Button type="submit" disabled={mutation.isPending || !canSave}>
            {mutation.isPending && (
              <Loader2 className="animate-spin" aria-hidden="true" />
            )}
            {t('common.save')}
          </Button>
        </DialogFooter>
      </div>
    </form>
  )
}

interface HealthNumberProps {
  id: string
  label: string
  placeholder: string
  value: string
  onChange: (value: string) => void
}

/** A digits-only field for the health tab. Empty means "inherit the default",
 *  so it never coerces to 0 — the placeholder shows what will be used. */
function HealthNumber({ id, label, placeholder, value, onChange }: HealthNumberProps) {
  return (
    <div className="space-y-2">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        inputMode="numeric"
        placeholder={placeholder}
        value={value}
        onChange={(event) => onChange(event.target.value.replace(/[^0-9]/g, ''))}
      />
    </div>
  )
}

interface ToggleRowProps {
  id: string
  label: string
  checked: boolean
  onChange: (checked: boolean) => void
}

function ToggleRow({ id, label, checked, onChange }: ToggleRowProps) {
  return (
    <div className="flex items-center justify-between gap-4">
      <Label htmlFor={id} className="font-normal">
        {label}
      </Label>
      <Switch id={id} checked={checked} onCheckedChange={onChange} />
    </div>
  )
}
