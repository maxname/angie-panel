import { useMutation } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { api, ApiError, type LoginRequest } from '@/lib/api'

interface LoginFormProps {
  onSuccess: () => void
}

export function LoginForm({ onSuccess }: LoginFormProps) {
  const { t } = useTranslation()
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')

  const mutation = useMutation({
    mutationFn: (body: LoginRequest) => api.login(body),
    onSuccess,
  })

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    mutation.mutate({ email, password })
  }

  const errorMessage = (() => {
    if (!mutation.isError) {
      return null
    }
    const error = mutation.error
    if (error instanceof ApiError && error.status === 401) {
      return t('login.invalidCredentials')
    }
    if (error instanceof ApiError) {
      return error.message
    }
    return t('common.error')
  })()

  return (
    <form className="grid gap-4" onSubmit={handleSubmit}>
      <div className="grid gap-2">
        <Label htmlFor="login-email">{t('login.email')}</Label>
        <Input
          id="login-email"
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
        <Label htmlFor="login-password">{t('login.password')}</Label>
        <Input
          id="login-password"
          type="password"
          autoComplete="current-password"
          required
          value={password}
          onChange={(event) => setPassword(event.target.value)}
        />
      </div>
      {errorMessage !== null && (
        <p role="alert" className="text-sm text-destructive">
          {errorMessage}
        </p>
      )}
      <Button type="submit" disabled={mutation.isPending}>
        {mutation.isPending ? t('login.submitting') : t('login.submit')}
      </Button>
    </form>
  )
}
