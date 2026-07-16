import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { LoginForm } from '@/components/login-form'
import { LanguageMenu, ThemeMenu } from '@/components/preference-menus'
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
    <div className="relative flex min-h-svh flex-col items-center justify-center gap-6 bg-muted/40 p-4">
      {/* Same corner as the app header. Both preferences matter most here:
          this is where someone lands in a language they can't read, or gets a
          white flash at 2am, with no shell to fix it from. */}
      <div className="absolute top-4 right-4 flex items-center gap-1">
        <LanguageMenu />
        <ThemeMenu />
      </div>
      <div className="flex items-center gap-2 text-lg font-semibold">
        <img src="/logo.png" alt="" className="size-5" />
        <span translate="no">{t('app.name')}</span>
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
