import { useNavigate } from '@tanstack/react-router'
import { Waypoints } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { LoginForm } from '@/components/login-form'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'

export function LoginPage() {
  const { t } = useTranslation()
  const navigate = useNavigate()

  return (
    <div className="flex min-h-svh flex-col items-center justify-center gap-6 bg-muted/40 p-4">
      <div className="flex items-center gap-2 text-lg font-semibold">
        <Waypoints className="size-5" aria-hidden="true" />
        {t('app.name')}
      </div>
      <Card className="w-full max-w-sm">
        <CardHeader>
          <CardTitle>{t('login.title')}</CardTitle>
          <CardDescription>{t('login.description')}</CardDescription>
        </CardHeader>
        <CardContent>
          <LoginForm onSuccess={() => void navigate({ to: '/' })} />
        </CardContent>
      </Card>
    </div>
  )
}
