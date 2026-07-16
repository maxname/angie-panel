import { useMutation } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { Waypoints } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { LanguageMenu, ThemeMenu } from '@/components/preference-menus'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api, ApiError } from '@/lib/api'

export function SetupPage() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [token, setToken] = useState('')
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [confirmPassword, setConfirmPassword] = useState('')
  const [passwordMismatch, setPasswordMismatch] = useState(false)

  const mutation = useMutation({
    mutationFn: async () => {
      await api.setup({ token, email, password })
      // Auto-login with the freshly created account.
      await api.login({ email, password })
    },
    onSuccess: () => void navigate({ to: '/' }),
  })

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (password !== confirmPassword) {
      setPasswordMismatch(true)
      return
    }
    setPasswordMismatch(false)
    mutation.mutate()
  }

  const errorMessage = (() => {
    if (passwordMismatch) {
      return t('setup.passwordMismatch')
    }
    if (!mutation.isError) {
      return null
    }
    const error = mutation.error
    // The backend returns 403 for an invalid/expired token or a disabled setup
    // path (never 401), so match 403 to show the localized hint.
    if (error instanceof ApiError && error.status === 403) {
      return t('setup.invalidToken')
    }
    if (error instanceof ApiError) {
      return error.message
    }
    return t('common.error')
  })()

  return (
    <div className="relative flex min-h-svh flex-col items-center justify-center gap-6 bg-muted/40 p-4">
      {/* Same corner as the app header. Both preferences matter most here:
          this is where someone lands in a language they can't read, or gets a
          white flash at 2am, with no shell to fix it from. */}
      <div className="absolute top-4 right-4 flex items-center gap-1">
        <LanguageMenu />
        <ThemeMenu />
      </div>
      <div className="flex items-center gap-2 text-lg font-semibold">
        <Waypoints className="size-5" aria-hidden="true" />
        <span translate="no">{t('app.name')}</span>
      </div>
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle>{t('setup.title')}</CardTitle>
          <CardDescription>{t('setup.description')}</CardDescription>
        </CardHeader>
        <CardContent>
          <form className="grid gap-4" onSubmit={handleSubmit}>
            <div className="grid gap-2">
              <Label htmlFor="setup-token">{t('setup.token')}</Label>
              <Input
                id="setup-token"
                type="text"
                autoComplete="off"
                required
                value={token}
                onChange={(event) => setToken(event.target.value)}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="setup-email">{t('setup.email')}</Label>
              <Input
                id="setup-email"
                type="email"
                autoComplete="email"
                spellCheck={false}
                autoCapitalize="off"
                required
                value={email}
                onChange={(event) => setEmail(event.target.value)}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="setup-password">{t('setup.password')}</Label>
              <Input
                id="setup-password"
                type="password"
                autoComplete="new-password"
                required
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="setup-confirm-password">{t('setup.confirmPassword')}</Label>
              <Input
                id="setup-confirm-password"
                type="password"
                autoComplete="new-password"
                required
                value={confirmPassword}
                onChange={(event) => setConfirmPassword(event.target.value)}
              />
            </div>
            {errorMessage !== null && (
              <p role="alert" className="text-sm text-destructive">
                {errorMessage}
              </p>
            )}
            <Button type="submit" disabled={mutation.isPending}>
              {mutation.isPending ? t('setup.submitting') : t('setup.submit')}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}
